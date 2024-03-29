use anyhow::{Result, anyhow, bail, Context};
use base64::{Engine as _, engine::general_purpose};
use clap::{Parser, ValueEnum};
use font::GenericFontFamily;
use indoc::formatdoc;
use lol_html::{element, HtmlRewriter, Settings, html_content::ContentType};
use mimalloc::MiMalloc;
use mobi::Mobi;
use regex::Regex;
use roxmltree::Document;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{self, Seek, Read, Write};
use std::panic;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
use std::sync::{Arc, Mutex};
use tracing_subscriber::EnvFilter;
use tracing::debug;
use zip::result::ZipError;

mod css;
mod font;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(ValueEnum, Clone, Debug)]
#[allow(non_camel_case_types)]
enum TextFragmentsPolyfill {
    none,
    inline,
    unpkg,
}

#[derive(Parser, Debug)]
#[clap(name = "unbook", version)]
/// Convert an ebook to a self-contained HTML file
struct ConvertCommand {
    /// The path to an .{epub,mobi,azw,azw3,lit,chm} file, or other format that Calibre
    /// can reasonably convert to HTMLZ. See https://manual.calibre-ebook.com/faq.html
    /// for a list of formats it supports, not all of which will convert nicely to HTMLZ.
    ebook_path: PathBuf,

    /// The path for the output .html file. If not specified, it is saved in the
    /// directory of the input file, with ".html" appended to the existing extension.
    #[clap(long, short = 'o')]
    output_path: Option<PathBuf>,

    /// Remove the ebook extension before appending ".html".
    ///
    /// This is not the default because it makes it harder to find the original
    /// ebook file when viewing the .html, and because you may have e.g. both .mobi
    /// and .epub with the same name in a directory.
    #[clap(long, short = 'e')]
    remove_ebook_ext: bool,

    /// Replace the output .html file if it already exists.
    #[clap(long, short = 'f')]
    force: bool,

    /// The base font-size (with a CSS unit) to use for the book text
    //
    // Tested: iPhone 11 & low-DPI laptop with Chrome; 15px seems like a better size than
    // the slightly-too-large 16px default, with good zoom increments in both directions.
    #[clap(long, default_value = "15px")]
    base_font_size: String,

    /// The base font-family to use for the book text
    //
    // Many books have no font-family in the CSS at all, and we want to use something better
    // than the default font chosen by iOS Safari (Times).
    #[clap(long, default_value = "sans-serif")]
    base_font_family: String,

    /// The monospace font-family to use
    #[clap(long, default_value = "monospace")]
    monospace_font_family: String,

    /// Font stack replacement mode for serif + sans-serif font stacks, treated as one set.
    /// The default mode "if-one" replaces fonts when there is just one distinct font stack.
    #[clap(long, default_value = "if-one")]
    replace_serif_and_sans_serif: css::FontFamilyReplacementMode,

    /// Font stack replacement mode for monospace font stacks.
    /// The default mode "if-one" replaces fonts when there is just one distinct font stack.
    #[clap(long, default_value = "if-one")]
    replace_monospace: css::FontFamilyReplacementMode,

    /// The minimum font-size (with a CSS unit) to use for the book text. This can be used
    /// to work around issues with bad 'em' sizing making fonts far too small.
    #[clap(long, default_value = "13px")]
    min_font_size: String,

    /// The max-width (with a CSS unit) to use for the book text
    #[clap(long, default_value = "5in")]
    max_width: String,

    /// The minimum line-height (with an optional CSS unit) to use for the book text
    // 1.5 is bad with a font size of 15px because 15 * 1.5 = 22.5,
    // which creates irregular line heights.
    #[clap(long, default_value = "1.53333333")]
    min_line_height: String,

    /// The CSS size of the inside margin for book text, when the viewport is wide
    /// enough to show the outside margin.
    #[clap(long, default_value = "32px")]
    inside_margin_when_wide: String,

    /// The CSS size of the inside margin for book text, when the viewport is not
    /// wide enough to show the outside margin.
    #[clap(long, default_value = "16px")]
    inside_margin_when_narrow: String,

    /// Background color (any CSS color) to use on the outside margin of the book,
    /// i.e. the <html> where there is no text. Use "unset" for no color.
    #[clap(long, default_value = "#888")]
    outside_bgcolor: String,

    /// Background color (any CSS color) to use for the text of the book, i.e. the <body>.
    /// Use "unset" for no color.
    #[clap(long, default_value = "#e9e9e9")]
    inside_bgcolor: String,

    /// ebooks are sometimes wrapped in an element with a white or near-white background
    /// color that effectively overrides unbook's inside_bgcolor. This similarity threshold
    /// is used when considering whether to replace these background colors: if R, G, and B
    /// of inside_bgcolor are all within R, G, and B of inside_bgcolor_similarity_threshold,
    /// the unwanted background-color is removed. Set to 0 to never replace, or 1 to always
    /// replace.
    #[clap(long, default_value = "0.2")]
    inside_bgcolor_similarity_threshold: f64,

    /// Additional HTML to append to <head> in the output HTML
    #[clap(long, default_value = "")]
    append_head: String,

    /// Path to the Calibre "ebook-convert" executable to use
    #[clap(long, default_value = "ebook-convert")]
    ebook_convert: String,

    /// Keep the temporary HTMLZ for debugging purposes
    #[clap(long)]
    keep_temporary_htmlz: bool,

    /// Which type of Text Fragments polyfill to add (if any) for the benefit
    /// of Firefox and Safari < 16.1 users
    #[clap(long, default_value = "inline")]
    text_fragments_polyfill: TextFragmentsPolyfill,

    /// Space-separated entries to add to Content-Security-Policy default-src
    #[clap(long, default_value = "")]
    csp_default_src: String,

    /// Space-separated entries to add to Content-Security-Policy font-src
    #[clap(long, default_value = "")]
    csp_font_src: String,

    /// Space-separated entries to add to Content-Security-Policy img-src
    #[clap(long, default_value = "")]
    csp_img_src: String,

    /// Space-separated entries to add to Content-Security-Policy style-src
    #[clap(long, default_value = "")]
    csp_style_src: String,

    /// Space-separated entries to add to Content-Security-Policy media-src
    #[clap(long, default_value = "")]
    csp_media_src: String,

    /// Space-separated entries to add to Content-Security-Policy script-src
    #[clap(long, default_value = "")]
    csp_script_src: String,

    /// Space-separated entries to add to Content-Security-Policy object-src
    #[clap(long, default_value = "")]
    csp_object_src: String,
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

#[derive(Debug)]
struct ZipReadTracker<R> {
    pub archive: zip::ZipArchive<R>,
    pub unread_files: HashSet<String>,
    pub missing_files: HashSet<String>,
}

impl<R: Read + Seek> ZipReadTracker<R> {
    fn new(archive: zip::ZipArchive<R>) -> Self {
        let unread_files: HashSet<String> = archive
            .file_names()
            .filter(|name| !(name.ends_with('/') || name.ends_with('\\')))
            .map(String::from)
            .collect();
        let missing_files = HashSet::new();
        ZipReadTracker {
            archive,
            unread_files,
            missing_files,
        }
    }

    fn get_content(&mut self, fname: &str) -> Result<Option<Vec<u8>>> {
        match self.archive.by_name(fname) {
            Err(ZipError::FileNotFound) => {
                self.missing_files.insert(fname.to_string());
                Ok(None)
            },
            Err(e) => bail!(e),
            Ok(mut entry) => {
                let mut vec = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut vec)?;
                self.unread_files.remove(fname);
                Ok(Some(vec))
            }
        }
    }
}

fn sort_join_hashset(hs: &HashSet<String>, sep: &str) -> String {
    let mut v: Vec<String> = hs.iter().cloned().collect::<Vec<_>>();
    v.sort();
    v.join(sep)
}

// Thanks to Anton Bukov in https://stackoverflow.com/a/59211505
fn catch_unwind_silent<F: FnOnce() -> R + panic::UnwindSafe, R>(f: F) -> std::thread::Result<R> {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(f);
    panic::set_hook(prev_hook);
    result
}

fn convert_file(command: ConvertCommand) -> Result<()> {
    let ConvertCommand {
        ebook_path,
        output_path,
        remove_ebook_ext,
        force,
        base_font_size,
        base_font_family,
        monospace_font_family,
        replace_serif_and_sans_serif,
        replace_monospace,
        min_font_size,
        max_width,
        min_line_height,
        inside_margin_when_wide,
        inside_margin_when_narrow,
        outside_bgcolor,
        inside_bgcolor,
        inside_bgcolor_similarity_threshold,
        append_head,
        ebook_convert,
        keep_temporary_htmlz,
        text_fragments_polyfill,
        csp_default_src,
        csp_font_src,
        csp_img_src,
        csp_style_src,
        csp_media_src,
        csp_script_src,
        csp_object_src,
    } = command;

    let output_path = match output_path {
        Some(p) => p,
        None => {
            if remove_ebook_ext {
                ebook_path.with_extension("html")
            } else {
                let mut filename = ebook_path.clone().into_os_string();
                filename.push(".html");
                ebook_path.with_file_name(filename)
            }
        }
    };
    // If needed, bail out early before running ebook-convert
    if output_path.exists() && !force {
        bail!("output file {:?} already exists; use unbook -f if you want to overwrite", output_path);
    }
    let first_4k = {
        let mut buf = [0; 4096];
        let mut ebook_file = fs::File::open(&ebook_path)
            .context("failed to open input file; are the path and permissions correct?")?;
        _ = ebook_file.read(&mut buf)
            .context("failed to read input file")?;
        buf
    };
    if first_4k.starts_with(b"<!DOCTYPE html>\n<html><head><!--\n\tebook converted to HTML with unbook ") {
        bail!("input file {ebook_path:?} was produced by unbook, refusing to convert it");
    }
    if infer::archive::is_pdf(&first_4k) {
        bail!("input file {ebook_path:?} is a PDF, refusing to create a poor HTML conversion");
    }
    if infer::book::is_mobi(&first_4k) {
        // https://github.com/vv9k/mobi-rs/issues/42
        // If it panics, we don't get an Ok(...) and we just ignore it.
        if let Ok(result) = catch_unwind_silent(|| {
            // mobi-rs might not be able to parse every MOBI; just skip the AZW4 check if it fails
            if let Ok(mobi) = Mobi::from_path(&ebook_path) {
                for record in mobi.raw_records() {
                    if record.content.starts_with(b"%MOP") {
                        bail!("input file {ebook_path:?} is a MOBI with a PDF inside, \
                               possibly an AZW4 Print Replica, refusing to create a poor HTML conversion");
                    }
                }
            }
            Ok(())
        }) {
            result?;
        };
    }

    let output_htmlz = {
        let random: String = std::iter::repeat_with(fastrand::alphanumeric).take(12).collect();
        env::temp_dir().join(format!("unbook-{random}.htmlz"))
    };
    let ebook_file_size = {
        let ebook_file = fs::File::open(&ebook_path)
            .context("failed to open input file; are the path and permissions correct?")?;
        let metadata = File::metadata(&ebook_file)
            .context("failed to get metadata for input file")?;
        metadata.len()
    };

    let mut command = Command::new(ebook_convert);
    command.env_clear();
    command.args([
        &ebook_path,
        &output_htmlz,
        // We need -vv for calibre to output its version
        &PathBuf::from("-vv"),
        // We have our own padding/margin and don't need Calibre's extra 5pt margin
        &PathBuf::from("--margin-top=0"),
        &PathBuf::from("--margin-bottom=0"),
        &PathBuf::from("--margin-left=0"),
        &PathBuf::from("--margin-right=0"),
        // We have our own minimum line-height implemented with a CSS variable
        &PathBuf::from("--minimum-line-height=0"),
    ]);
    // Just .env_clear() is fine on Linux, but Python on Windows requires at least SystemRoot
    // to be present to avoid this:
    //
    // Fatal Python error: _Py_HashRandomization_Init: failed to get random numbers to initialize Python
    // Python runtime state: preinitialized
    //
    // On macOS, we need to retain PATH for the default "ebook-convert" to work.
    for (name, value) in ["SystemDrive", "SystemRoot", "TEMP", "TMP", "PATH"]
        .iter()
        .filter_map(|name| env::var(name).ok().map(|value| (name, value)))
    {
        command.env(name, value);
    }
    let calibre_output = command.output()
        .context("failed to run Calibre ebook-convert: is a directory with ebook-convert \
                  in your PATH? (see also \"--ebook-convert\" in unbook --help)")?;
    if !calibre_output.status.success() {
        let stderr = String::from_utf8_lossy(&calibre_output.stderr);
        match calibre_output.status.code() {
            None       => bail!("ebook-convert was terminated by a signal:\n\n{stderr}"),
            Some(code) => bail!("ebook-convert failed with exit status {code}:\n\n{stderr}"),
        };
    }

    let htmlz_file = fs::File::open(&output_htmlz)
        .with_context(|| format!("ebook-convert succeeded, but the HTMLZ file at {output_htmlz:?} could not be opened"))?;
    let archive = zip::ZipArchive::new(htmlz_file)
        .with_context(|| format!("failed to read the HTMLZ file at {output_htmlz:?} or parse it as a ZIP file"))?;
    let filenames: Vec<&str> = archive.file_names().collect();
    debug!(filenames = ?filenames, "files inside htmlz");
    let mut zip = ZipReadTracker::new(archive);

    let html = zip.get_content("index.html")?
        .ok_or_else(|| anyhow!("index.html not found in HTMLZ"))?;
    if !html.starts_with(b"<html><head>") {
        bail!("index.html in HTMLZ does not start with <html><head>");
    }

    let calibre_css = String::from_utf8(
        zip.get_content("style.css")?
        .ok_or_else(|| anyhow!("style.css not found in HTMLZ"))?
    ).context("failed to parse style.css in HTMLZ as UTF-8")?;

    let metadata = String::from_utf8(
        zip.get_content("metadata.opf")?
        .ok_or_else(|| anyhow!("metadata.opf not found in HTMLZ"))?
    ).context("failed to parse metadata.opf in HTMLZ as UTF-8")?;
    let metadata_doc = parse_xml(&metadata)
        .context("failed to parse metadata.opf in HTMLZ as XML")?;

    let cover_fname = get_cover_filename(&metadata_doc);
    let mut cover = None;
    if let Some(cover_fname) = &cover_fname {
        cover = Some(
            zip.get_content(cover_fname)?
            .ok_or_else(|| anyhow!("{cover_fname} not found in HTMLZ"))?
        );
    }

    let mut output = Vec::with_capacity(html.len() * 4);
    let zip_arc = Arc::new(Mutex::new(zip));
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                // Prepend the book cover image to the body
                element!("body", |el| {
                    let skip_cover = "<a id=\"unbook-skip-cover\"></a>";
                    if let Some(cover_fname) = cover_fname.as_ref() {
                        let mime_type = get_mime_type(cover_fname)
                            .with_context(|| format!("failed to determine mime type for file {cover_fname:?} in HTMLZ"))?;
                        let image_base64 = general_purpose::STANDARD.encode(cover.as_ref().unwrap());
                        let inline_src = format!("data:{mime_type};base64,{image_base64}");
                        let extra_body = formatdoc!("
                            \n<img class=\"unbook-cover\" alt=\"Book cover\" src=\"{inline_src}\" />
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
                    if let Some(image) = zip.get_content(&src)? {
                        let mime_type = get_mime_type(&src)
                            .with_context(|| format!("failed to determine mime type for file {src:?} in HTMLZ"))?;
                        let image_base64 = general_purpose::STANDARD.encode(image);
                        let inline_src = format!("data:{mime_type};base64,{image_base64}");
                        el.set_attribute("src", &inline_src)?;
                        // Make the HTML source a little easier to read by putting inline images on their own lines
                        el.before("<!--\n-->", ContentType::Html);
                        el.after("<!--\n-->", ContentType::Html);
                    }
                    Ok(())
                }),
                // https://developer.mozilla.org/en-US/docs/Web/SVG/Element/image
                element!("image[href]", |el| {
                    let href = el.get_attribute("href").unwrap();
                    let mut zip = zip_arc.lock().unwrap();
                    if let Some(image) = zip.get_content(&href)? {
                        let mime_type = get_mime_type(&href)
                            .with_context(|| format!("failed to determine mime type for file {href:?} in HTMLZ"))?;
                        let image_base64 = general_purpose::STANDARD.encode(image);
                        let inline_href = format!("data:{mime_type};base64,{image_base64}");
                        el.set_attribute("href", &inline_href)?;
                    }
                    Ok(())
                }),
                // Delete reference to style.css
                element!(r#"link[href="style.css"][rel="stylesheet"][type="text/css"]"#, |el| {
                    el.remove();
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
    if !keep_temporary_htmlz {
        fs::remove_file(&output_htmlz)
            .with_context(|| format!("failed to remove temporary HTMLZ file at {output_htmlz:?}"))?;
    }

    let fro = css::FontReplacementOptions {
        min_font_size,
        base_font_size,
        base_font_family,
        monospace_font_family,
        replace_serif_and_sans_serif,
        replace_monospace,
    };

    // We do this outside and after lol-html because our <!-- header --> needs to contain
    // a list of files which were not read from the ZIP archive.
    let family_map = css::get_generic_font_family_map(&calibre_css);
    let extra_head = {
        let fixed_css = css::fix_css(&calibre_css, &fro, &family_map, &inside_bgcolor, inside_bgcolor_similarity_threshold);
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
        let calibre_stderr_line_count = calibre_stderr.lines().count();
        let unbook_version = env!("CARGO_PKG_VERSION");
        let top_css = css::top_css(
            &fro,
            &max_width,
            &min_line_height,
            &inside_margin_when_wide,
            &inside_margin_when_narrow,
            &outside_bgcolor,
            &inside_bgcolor,
        );
        let (unread_files_count, unread_files_text) = {
            let zip = zip_arc.lock().unwrap();
            let mut unread_files: Vec<String> = zip.unread_files.iter().cloned().collect();
            unread_files.sort();
            (
                unread_files.len(),
                indent("\t\t", &escape_html_comment_close(&unread_files.join("\n")))
            )
        };
        let (missing_files_count, missing_files_text) = {
            let zip = zip_arc.lock().unwrap();
            let mut missing_files: Vec<String> = zip.missing_files.iter().cloned().collect();
            missing_files.sort();
            (
                missing_files.len(),
                indent("\t\t", &escape_html_comment_close(&missing_files.join("\n")))
            )
        };
        let text_fragments_js = include_str!("text-fragments-polyfill.js");
        let text_fragments_polyfill = match text_fragments_polyfill {
            TextFragmentsPolyfill::none => String::new(),
            TextFragmentsPolyfill::inline => formatdoc!("

                <script type=\"module\">
                {text_fragments_js}
                </script>
            "),
            TextFragmentsPolyfill::unpkg => formatdoc!("

                <script type=\"module\">
                if (!('fragmentDirective' in Location.prototype) && !('fragmentDirective' in document)) {{
                    import('https://unpkg.com/text-fragments-polyfill');
                }}
                </script>
            "),
        };
        // Don't let the book reference any external scripts, images, or other resources
        let csp = formatdoc!("
            <meta http-equiv=\"Content-Security-Policy\" content=\"
                default-src 'none' {csp_default_src};
                font-src 'self' data: {csp_font_src};
                img-src 'self' data: {csp_img_src};
                style-src 'unsafe-inline' {csp_style_src};
                media-src 'self' data: {csp_media_src};
                script-src 'unsafe-inline' data: {csp_script_src};
                object-src 'self' data: {csp_object_src};
            \">"
        );

        let empty = &HashSet::new();

        let font_stacks_unknown    = family_map.get(&None).unwrap_or(empty);
        let font_stacks_serif      = family_map.get(&Some(GenericFontFamily::Serif)).unwrap_or(empty);
        let font_stacks_sans_serif = family_map.get(&Some(GenericFontFamily::SansSerif)).unwrap_or(empty);
        let font_stacks_monospace  = family_map.get(&Some(GenericFontFamily::Monospace)).unwrap_or(empty);
        let font_stacks_fantasy    = family_map.get(&Some(GenericFontFamily::Fantasy)).unwrap_or(empty);
        let font_stacks_cursive    = family_map.get(&Some(GenericFontFamily::Cursive)).unwrap_or(empty);

        let font_stacks_unknown_count    = font_stacks_unknown.len();
        let font_stacks_serif_count      = font_stacks_serif.len();
        let font_stacks_sans_serif_count = font_stacks_sans_serif.len();
        let font_stacks_monospace_count  = font_stacks_monospace.len();
        let font_stacks_fantasy_count    = font_stacks_fantasy.len();
        let font_stacks_cursive_count    = font_stacks_cursive.len();

        let font_stacks_unknown_text    = indent("\t\t\t", &escape_html_comment_close(&sort_join_hashset(font_stacks_unknown, "\n")));
        let font_stacks_serif_text      = indent("\t\t\t", &escape_html_comment_close(&sort_join_hashset(font_stacks_serif, "\n")));
        let font_stacks_sans_serif_text = indent("\t\t\t", &escape_html_comment_close(&sort_join_hashset(font_stacks_sans_serif, "\n")));
        let font_stacks_monospace_text  = indent("\t\t\t", &escape_html_comment_close(&sort_join_hashset(font_stacks_monospace, "\n")));
        let font_stacks_fantasy_text    = indent("\t\t\t", &escape_html_comment_close(&sort_join_hashset(font_stacks_fantasy, "\n")));
        let font_stacks_cursive_text    = indent("\t\t\t", &escape_html_comment_close(&sort_join_hashset(font_stacks_cursive, "\n")));

        // If you change the header: YOU MUST ALSO UPDATE first_4k.starts_with above
        formatdoc!("<!--
            \tebook converted to HTML with unbook {unbook_version}

            \toriginal file name: {ebook_basename}
            \toriginal file size: {ebook_file_size}

            \tmetadata.opf:
            {metadata_}
            \tHTMLZ files which were discarded because they were not referenced by the HTML (count: {unread_files_count}):
            {unread_files_text}
            \tnote: if this is just one image, it is typically because Calibre erroneously duplicated the cover image.

            \tfiles which were referenced but missing in the HTMLZ (count: {missing_files_count}):
            {missing_files_text}

            \tfont stacks:
            \t\tunknown (count: {font_stacks_unknown_count}):
            {font_stacks_unknown_text}
            \t\tserif (count: {font_stacks_serif_count}):
            {font_stacks_serif_text}
            \t\tsans-serif (count: {font_stacks_sans_serif_count}):
            {font_stacks_sans_serif_text}
            \t\tmonospace (count: {font_stacks_monospace_count}):
            {font_stacks_monospace_text}
            \t\tfantasy (count: {font_stacks_fantasy_count}):
            {font_stacks_fantasy_text}
            \t\tcursive (count: {font_stacks_cursive_count}):
            {font_stacks_cursive_text}

            \tcalibre stderr output (lines: {calibre_stderr_line_count}):
            {calibre_stderr}

            \tcalibre conversion log:
            {calibre_log}
            -->
            {csp}
            <!-- viewport-fit=cover to prevent iOS Safari from applying the body background-color
                 to the \"safe area\": https://css-tricks.com/the-notch-and-css/ -->
            <meta name=\"viewport\" content=\"width=device-width, viewport-fit=cover\" />
            <meta name=\"referrer\" content=\"no-referrer\" />
            <style>
            {top_css}

            {fixed_css}
            </style>
            {text_fragments_polyfill}
            {append_head}
        ")
    };

    let mut output_file = if force {
        fs::File::create(&output_path)
            .with_context(|| format!("failed to open output file {output_path:?} for writing"))?
    } else {
        // Repeat the early check with the same error message
        if output_path.exists() {
            bail!("output file {:?} already exists; use unbook -f if you want to overwrite", output_path);
        }
        // TODO: use fs::File::create_new once stable
        create_new(&output_path)
            .with_context(|| format!("failed to open output file {output_path:?} for writing"))?
    };
    // Add a doctype because there probably isn't any reason for us to be in quirks mode
    // If you change the header: YOU MUST ALSO UPDATE first_4k.starts_with above
    output_file.write_all(b"<!DOCTYPE html>\n<html><head>")?;
    output_file.write_all(extra_head.as_bytes())?;
    let html_head = b"<html><head>";
    assert!(output.starts_with(html_head));
    output_file.write_all(&output[html_head.len()..])?;

    Ok(())
}


fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("warn"))
        .unwrap();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .init();

    let command = ConvertCommand::parse();
    let ConvertCommand { ebook_path, .. } = &command;
    let ebook_path = ebook_path.clone();
    convert_file(command)
        .with_context(|| format!("failed to convert input file {ebook_path:?}"))?;

    Ok(())
}
