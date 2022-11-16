use clap::Parser;
use mimalloc::MiMalloc;
use zip::ZipArchive;
use std::str;
use std::{fs::{self, File}, io::{Write, Read}, collections::HashMap};
use tracing::debug;
use tracing_subscriber::EnvFilter;
use anyhow::{Result, anyhow, bail};
use std::io;
use std::path::Path;
use std::{process::Command, path::PathBuf};
use lol_html::{element, HtmlRewriter, Settings, html_content::ContentType};
use indoc::formatdoc;
use regex::Regex;
use roxmltree::Document;

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

fn get_zip_content(archive: &mut ZipArchive<File>, fname: &str) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(fname)?;
    let mut vec = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut vec)?;
    Ok(vec)
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

fn fix_css(css: &str) -> String {
    let re = Regex::new(r"(?m)^(?P<indent>\s*)line-height:\s*(?P<height>[^;]+?);?$").unwrap();
    let out = re.replace_all(css, "${indent}line-height: max($height, var(--min-line-height));").into();
    out
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
    let calibre_output = Command::new(ebook_convert)
        .env_clear()
        // We need -vv for calibre to output its version
        .args([&ebook_path, &output_htmlz, &PathBuf::from("-vv")])
        .output()?;

    let htmlz_file = fs::File::open(&output_htmlz).unwrap();
    let mut archive = zip::ZipArchive::new(htmlz_file)?;
    let filenames: Vec<&str> = archive.file_names().collect();
    debug!(filenames = ?filenames, "files inside htmlz");

    let html = get_zip_content(&mut archive, "index.html")?;
    let calibre_css = String::from_utf8(get_zip_content(&mut archive, "style.css")?)?;
    let metadata = String::from_utf8(get_zip_content(&mut archive, "metadata.opf")?)?;
    let metadata_doc = parse_xml(&metadata)?;
    let cover_fname = get_cover_filename(&metadata_doc);
    let mut cover = None;
    if let Some(cover_fname) = &cover_fname {
        cover = Some(get_zip_content(&mut archive, cover_fname)?);
    }
    let mut output = Vec::with_capacity(html.len() * 4);
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!("head", |el| {
                    let top_css = formatdoc!("
                        :root {{
                            --min-line-height: {min_line_height};
                        }}

                        img {{
                            /* We don't want to let images widen the page, especially on mobile.
                             *
                             * TODO: allow images to exceed the width of the container, but
                             * not the viewport width, and without widening the viewport.
                             * 
                             * https://stackoverflow.com/a/41059954 does not work because it
                             * widens the page when there is a wide image. */
                            max-width: 100%;
                        }}

                        body {{
                            max-width: {max_width};
                            margin: 0 auto;
                            padding: 1em;
                            line-height: var(--min-line-height);
                            /* Without word-break: break-word, iOS Safari 16.1 lets
                             * very long words e.g. URLs widen the page */
                            word-break: break-word;
                        }}
                    ");
                    let fixed_css = fix_css(&calibre_css);
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
                    // If you change the header: YOU MUST ALSO UPDATE is_file_an_unbook_conversion
                    let extra_head = formatdoc!("<!--
                        \tebook converted to HTML with unbook {unbook_version}
                        \toriginal file: {ebook_basename}
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
                    let image = get_zip_content(&mut archive, &src)?;
                    let mime_type = get_mime_type(&src)?;
                    let image_base64 = base64::encode(image);
                    let inline_src = format!("data:{mime_type};base64,{image_base64}");
                    el.set_attribute("src", &inline_src)?;
                    // Make the HTML source a little easier to read by putting inline images on their own lines
                    el.before("<!--\n-->", ContentType::Html);
                    el.after("<!--\n-->", ContentType::Html);
                    Ok(())
                })
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_fix_css() {
        let input = "
            .something {
                line-height: 1.2
            }

            .something-else {
                line-height: 1.3;
                font-size: 14pt
            }
        ";

        let output = "
            .something {
                line-height: max(1.2, var(--min-line-height));
            }

            .something-else {
                line-height: max(1.3, var(--min-line-height));
                font-size: 14pt
            }
        ";

        assert_eq!(fix_css(input), output);
    }
}
