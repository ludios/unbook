use clap::Parser;
use mimalloc::MiMalloc;
use std::fs;
use tracing::info;
use tracing_subscriber::EnvFilter;
use anyhow::{Result, bail};
use std::{process::{Command, Stdio}, ffi::OsString, path::PathBuf};

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
        None => ebook_path.with_extension("htmlz"),
    };
    // If needed, bail out early before running ebook-convert
    if output_path.exists() && !replace {
        bail!("{:?} already exists", output_path);
    }

    let output_htmlz = {
        let random: String = std::iter::repeat_with(fastrand::alphanumeric).take(12).collect();
        std::env::temp_dir().join(format!("unbook-{random}.htmlz"))
    };
    let status = Command::new(ebook_convert)
        .stdin(Stdio::null())
        .env_clear()
        .args([&ebook_path, &output_htmlz])
        .status()?;
    let code = status.code();
    match code {
        None => { bail!("ebook-convert was terminated by a signal"); }
        Some(code) if code != 0 => { bail!("ebook-convert returned exit code {code}"); }
        Some(_) => {}
    }
    
    let htmlz_file = fs::File::open(&output_htmlz).unwrap();
    let archive = zip::ZipArchive::new(htmlz_file)?;
    let filenames: Vec<&str> = archive.file_names().collect();
    println!("filenames: {:#?}", filenames);

    Ok(())
}
