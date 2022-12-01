use clap::Parser;
use mimalloc::MiMalloc;
use std::collections::HashSet;
use std::os::unix::prelude::MetadataExt;
use std::str;
use std::sync::{Arc, Mutex};
use std::{fs::{self, File}, io::{Write, Read}, collections::HashMap};
use tracing::debug;
use tracing_subscriber::EnvFilter;
use anyhow::{Result, anyhow, bail};
use std::io::{self, Seek};
use std::path::Path;
use std::{process::Command, path::PathBuf};
use lol_html::{element, HtmlRewriter, Settings, html_content::ContentType};
use regex::Regex;
use roxmltree::Document;
use indoc::formatdoc;

mod css;
mod font;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug)]
#[clap(name = "unbook", version)]
/// Convert an ebook to a single HTML file
struct ConvertCommand {
    /// The path to an .epub, .mobi, .azw3 file, or other format that Calibre can
    /// reasonably convert to HTMLZ. See https://manual.calibre-ebook.com/faq.html
    /// for a list of formats it supports, not all of which will convert nicely to HTMLZ.
    ebook_path: PathBuf,

    /// The path for the output .html file. If not specified, it is saved in the
    /// directory of the input file, with the ebook extension replaced with "html".
    #[clap(long, short = 'o')]
    output_path: Option<PathBuf>,

    /// Whether to replace the output .html file if it already exists.
    #[clap(long, short = 'f')]
    replace: bool,

    /// The base font-size (with a CSS unit) to use for the book text
    //
    // Tested: iPhone 11 & low-DPI laptop with Chrome; 15px seems like a better size than
    // the slightly-too-large 16px default, with good zoom increments in both directions.
    #[clap(long, default_value = "15px")]
    base_font_size: String,

    /// The minimum font-size (with a CSS unit) to use for the book text. This can be used
    /// to work around issues with bad 'em' sizing making fonts far too small.
    #[clap(long, default_value = "13px")]
    min_font_size: String,

    /// The max-width (with a CSS unit) to use for the book text
    #[clap(long, default_value = "33em")]
    max_width: String,

    /// The minimum line-height (with an optional CSS unit) to use for the book text
    #[clap(long, default_value = "1.5")]
    min_line_height: String,

    /// Path to the Calibre "ebook-convert" executable to use
    #[clap(long, default_value = "ebook-convert")]
    ebook_convert: String,
}

fn create_new<P: AsRef<Path>>(path: P) -> io::Result<File> {
    fs::OpenOptions::new().read(true).write(true).create_new(true).open(path.as_ref())
}

/// Filter a Calibre `ebook-convert -vv` stdout to remove the input path and output path
fn filter_calibre_log(log: &str) -> String {
    let mut out = String::with_capacity(log.len());
    let mut fix_next_line = false;
    for line in log.lines() {
        if fix_next_line {
            fix_next_line = false;
            if line.starts_with("on ") {
                out.push_str("on […]\n");
            }
        } else if line.starts_with("InputFormatPlugin: ") {
            fix_next_line = true;
            out.push_str(line);
            out.push('\n');
        } else if line.starts_with("HTMLZ output written to ") {
            out.push_str("HTMLZ output written to […]\n");
        } else if line.starts_with("Output saved to ") {
            out.push_str("Output saved to […]\n");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn escape_html_comment_close(s: &str) -> String {
    s.replace("-->", r"-[breaking up an \x2D\x2D\3E]->")
}

fn indent(indent: &str, text: &str) -> String {
    let re = Regex::new(r"(?m)^").unwrap();
    let out = re.replace_all(text, indent).into();
    out
}

/// Return a `roxmltree::Document` for some XML string
fn parse_xml(xml: &str) -> Result<Document<'_>> {
    let doc = Document::parse(xml)
        .map_err(|_| anyhow!("roxmltree could not parse XML: {:?}", xml))?;
    Ok(doc)
}

fn get_cover_filename(doc: &Document<'_>) -> Option<String> {
    let cover = doc.descendants().find(|node| node.tag_name().name() == "reference" && node.attribute("type") == Some("cover"));
    cover.and_then(|node| node.attribute("href")).map(String::from)
}

fn get_mime_type(filename: &str) -> Result<&'static str> {
    let mime_types = {
        let mut mime_types = HashMap::with_capacity(4);
        mime_types.insert("gif".to_string(), "image/gif");
        mime_types.insert("jpg".to_string(), "image/jpeg");
        mime_types.insert("jpeg".to_string(), "image/jpeg");
        mime_types.insert("png".to_string(), "image/png");
        mime_types.insert("svg".to_string(), "image/svg+xml");
        mime_types
    };

    let (_, ext) = filename.rsplit_once('.')
        .ok_or_else(|| anyhow!("no extension for src={filename}"))?;
    let ext = ext.to_ascii_lowercase();
    let mime_type = mime_types.get(&ext)
        .ok_or_else(|| anyhow!("no mimetype for extension {ext}"))?;
    Ok(mime_type)
}

fn is_file_an_unbook_conversion(path: &PathBuf) -> Result<bool> {
    let mut file = File::open(path)?;
    let unbook_header = b"<html><head><!--\n\tebook converted to HTML with unbook ";
    let mut buf = vec![0u8; unbook_header.len()];
    file.read_exact(&mut buf)?;
    Ok(buf == unbook_header)
}

#[derive(Debug)]
struct ZipReadTracker<R> {
    archive: zip::ZipArchive<R>,
    unread_files: HashSet<String>,
}

impl<R: Read + Seek> ZipReadTracker<R> {
    fn new(archive: zip::ZipArchive<R>) -> Self {
        let unread_files: HashSet<String> = archive.file_names().map(String::from).collect();
        ZipReadTracker {
            archive,
            unread_files
        }
    }

    fn get_content(&mut self, fname: &str) -> Result<Vec<u8>> {
        let mut entry = self.archive.by_name(fname)?;
        let mut vec = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut vec)?;
        self.unread_files.remove(fname);
        Ok(vec)
    }
}

fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("warn"))
        .unwrap();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .init();

    let ConvertCommand {
        ebook_path,
        output_path,
        replace,
        base_font_size,
        min_font_size,
        max_width,
        min_line_height,
        ebook_convert
    } = ConvertCommand::parse();

    let output_path = match output_path {
        Some(p) => p,
        None => ebook_path.with_extension("html"),
    };
    // If needed, bail out early before running ebook-convert
    if output_path.exists() && !replace {
        bail!("{:?} already exists", output_path);
    }
    if is_file_an_unbook_conversion(&ebook_path)? {
        bail!("input file {ebook_path:?} was produced by unbook, refusing to convert it");
    }

    let output_htmlz = {
        let random: String = std::iter::repeat_with(fastrand::alphanumeric).take(12).collect();
        std::env::temp_dir().join(format!("unbook-{random}.htmlz"))
    };
    let ebook_file_size = {
        let ebook_file = fs::File::open(&ebook_path)?;
        let metadata = File::metadata(&ebook_file)?;
        metadata.size()
    };
    let calibre_output = Command::new(ebook_convert)
        .env_clear()
        // We need -vv for calibre to output its version
        .args([&ebook_path, &output_htmlz, &PathBuf::from("-vv")])
        .output()?;

    let htmlz_file = fs::File::open(&output_htmlz).unwrap();
    let archive = zip::ZipArchive::new(htmlz_file)?;
    let filenames: Vec<&str> = archive.file_names().collect();
    debug!(filenames = ?filenames, "files inside htmlz");
    let mut zip = ZipReadTracker::new(archive);

    let html = zip.get_content("index.html")?;
    let calibre_css = String::from_utf8(zip.get_content("style.css")?)?;
    let metadata = String::from_utf8(zip.get_content("metadata.opf")?)?;
    let metadata_doc = parse_xml(&metadata)?;
    let cover_fname = get_cover_filename(&metadata_doc);
    let mut cover = None;
    if let Some(cover_fname) = &cover_fname {
        cover = Some(zip.get_content(cover_fname)?);
    }
    let mut output = Vec::with_capacity(html.len() * 4);
    let zip_arc = Arc::new(Mutex::new(zip));
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!("head", |el| {
                    let fixed_css = css::fix_css(&calibre_css);
                    let ebook_basename =
                        escape_html_comment_close(
                            &ebook_path.file_name().unwrap().to_string_lossy());
                    let metadata_ =
                            indent("\t\t",
                                &escape_html_comment_close(
                                    &metadata));
                    let calibre_log =
                        indent("\t\t",
                            &escape_html_comment_close(
                                &filter_calibre_log(
                                    &String::from_utf8_lossy(&calibre_output.stdout))));
                    // TODO: make sure we're not putting e.g. full file paths into the HTML via some stray stderr message
                    let calibre_stderr =
                        indent("\t\t",
                            &escape_html_comment_close(
                                &String::from_utf8_lossy(&calibre_output.stderr)));
                    let unbook_version = env!("CARGO_PKG_VERSION");
                    let top_css = css::top_css(&base_font_size, &min_font_size, &max_width, &min_line_height);
                    // If you change the header: YOU MUST ALSO UPDATE is_file_an_unbook_conversion
                    let extra_head = formatdoc!("<!--
                        \tebook converted to HTML with unbook {unbook_version}
                        \toriginal file name: {ebook_basename}
                        \toriginal file size: {ebook_file_size}
                        \tmetadata.opf:
                        {metadata_}
                        \tcalibre stderr output:
                        {calibre_stderr}

                        \tcalibre conversion log:

                        {calibre_log}
                        -->
                        <meta name=\"viewport\" content=\"width=device-width\" />
                        <meta name=\"referrer\" content=\"no-referrer\" />
                        <style>
                        {top_css}

                        {fixed_css}
                        </style>
                    ");
                    el.prepend(&extra_head, ContentType::Html);
                    Ok(())
                }),
                element!("body", |el| {
                    let skip_cover = "<a id=\"unbook-skip-cover\"></a>";
                    if let Some(cover_fname) = cover_fname.as_ref() {
                        let mime_type = get_mime_type(cover_fname)?;
                        let image_base64 = base64::encode(cover.as_ref().unwrap());
                        let inline_src = format!("data:{mime_type};base64,{image_base64}");
                        let extra_body = formatdoc!("
                            \n<img alt=\"Book cover\" src=\"{inline_src}\" />
                            {skip_cover}
                        ");
                        el.prepend(&extra_body, ContentType::Html);
                    } else {
                        el.prepend(skip_cover, ContentType::Html);
                    }
                    Ok(())
                }),
                element!("img[src]", |el| {
                    let src = el.get_attribute("src").unwrap();
                    let mut zip = zip_arc.lock().unwrap();
                    let image = zip.get_content(&src)?;
                    let mime_type = get_mime_type(&src)?;
                    let image_base64 = base64::encode(image);
                    let inline_src = format!("data:{mime_type};base64,{image_base64}");
                    el.set_attribute("src", &inline_src)?;
                    // Make the HTML source a little easier to read by putting inline images on their own lines
                    el.before("<!--\n-->", ContentType::Html);
                    el.after("<!--\n-->", ContentType::Html);
                    Ok(())
                }),
                // https://developer.mozilla.org/en-US/docs/Web/SVG/Element/image
                element!("image[href]", |el| {
                    let href = el.get_attribute("href").unwrap();
                    let mut zip = zip_arc.lock().unwrap();
                    let image = zip.get_content(&href)?;
                    let mime_type = get_mime_type(&href)?;
                    let image_base64 = base64::encode(image);
                    let inline_href = format!("data:{mime_type};base64,{image_base64}");
                    el.set_attribute("href", &inline_href)?;
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| output.extend_from_slice(c)
    );
    rewriter.write(&html)?;
    rewriter.end()?;

    // We're done reading the htmlz at this point
    fs::remove_file(&output_htmlz)?;

    let mut output_file = if replace {
        fs::File::create(&output_path)?
    } else {
        // TODO: use fs::File::create_new once stable
        create_new(&output_path).map_err(|_| anyhow!("{:?} already exists", output_path))?
    };
    output_file.write_all(&output)?;

    Ok(())
}
