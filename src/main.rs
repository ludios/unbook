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
use indoc::{formatdoc, indoc};

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
    output_path: Option<PathBuf>,

    /// Whether to replace the output .html file if it already exists.
    #[clap(long, short = 'f')]
    replace: bool,

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
    s.replace("-->", r"-[this was just \x2D\x2D\3E]->")
}

fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("warn"))
        .unwrap();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .init();

    let ConvertCommand { ebook_path, output_path, replace, ebook_convert } = ConvertCommand::parse();
    let output_path = match output_path {
        Some(p) => p,
        None => ebook_path.with_extension("html"),
    };
    // If needed, bail out early before running ebook-convert
    if output_path.exists() && !replace {
        bail!("{:?} already exists", output_path);
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
    if !calibre_output.stderr.is_empty() {
        bail!("calibre had stderr output: {:#?}", calibre_output.stderr);
    }

    let htmlz_file = fs::File::open(&output_htmlz).unwrap();
    let mut archive = zip::ZipArchive::new(htmlz_file)?;
    let filenames: Vec<&str> = archive.file_names().collect();
    debug!(filenames = ?filenames, "files inside htmlz");

    let mime_types = {
        let mut mime_types = HashMap::with_capacity(4);
        mime_types.insert("gif".to_string(), "image/gif");
        mime_types.insert("jpg".to_string(), "image/jpeg");
        mime_types.insert("jpeg".to_string(), "image/jpeg");
        mime_types.insert("png".to_string(), "image/png");
        mime_types.insert("svg".to_string(), "image/svg+xml");
        mime_types
    };

    let html = get_zip_content(&mut archive, "index.html")?;
    let style = String::from_utf8(get_zip_content(&mut archive, "style.css")?)?;
    let mut output = Vec::with_capacity(html.len() * 4);
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!("head", |el| {
                    let top_css = indoc!("
                        body {
                            max-width: 35em;
                            margin: 0 auto 0 auto;
                        }
                    ");
                    let ebook_basename =
                        escape_html_comment_close(
                            &ebook_path.file_name().unwrap().to_string_lossy());
                    let calibre_log =
                        escape_html_comment_close(
                            &filter_calibre_log(
                                &String::from_utf8_lossy(&calibre_output.stdout)));
                    let unbook_version = env!("CARGO_PKG_VERSION");
                    let extra_head = formatdoc!("<!--
                        \x20ebook converted to HTML with unbook
                        \x20original file: {ebook_basename}
                        \x20unbook version: {unbook_version}
                        \x20calibre conversion log:

                        {calibre_log}
                        -->
                        <meta name=\"viewport\" content=\"width=device-width\" />
                        <meta name=\"referrer\" content=\"no-referrer\" />
                        <style>
                        {top_css}

                        {style}
                        </style>
                    ", style = style);
                    el.prepend(&extra_head, ContentType::Html);
                    Ok(())
                }),
                element!("img[src]", |el| {
                    let src = el.get_attribute("src").unwrap();
                    let image = get_zip_content(&mut archive, &src)?;
                    let mime_type = {
                        let (_, ext) = src.rsplit_once('.')
                            .ok_or_else(|| anyhow!("no extension for src={src}"))?;
                        let ext = ext.to_ascii_lowercase();
                        let mime_type = mime_types.get(&ext)
                            .ok_or_else(|| anyhow!("no mimetype for extension {ext}"))?;
                        mime_type
                    };
                    let image_base64 = base64::encode(image);
                    let inline_src = format!("data:{mime_type};base64,{image_base64}");
                    el.set_attribute("src", &inline_src)?;
                    Ok(())
                })
            ],
            ..Settings::default()
        },
        |c: &[u8]| output.extend_from_slice(c)
    );
    rewriter.write(&html)?;
    rewriter.end()?;

    let mut output_file = if replace {
        fs::File::create(&output_path)?
    } else {
        // TODO: use fs::File::create_new once stable
        create_new(&output_path).map_err(|_| anyhow!("{:?} already exists", output_path))?
    };
    output_file.write_all(&output)?;

    Ok(())
}
