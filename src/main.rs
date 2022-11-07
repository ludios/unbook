use clap::Parser;
use mimalloc::MiMalloc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug)]
#[clap(name = "unbook", version)]
/// Convert an ebook to a single HTML file
struct ConvertCommand {
    /// The path to an .epub, .mobi, .azw3 file, or other format that Calibre can
    /// reasonably convert to HTMLZ. See https://manual.calibre-ebook.com/faq.html
    /// for a list of formats it supports, not all of which will convert nicely to HTMLZ.
    ebook_file: String,

    /// The path for the output .html file. If not specified, it is saved in the
    /// directory of the input file, with the ebook extension replaced with "html".
    output_file: String,

    /// Whether to replace the output .html file if it already exists.
    #[clap(long, short = 'f')]
    replace: bool,
}

fn main() {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("warn"))
        .unwrap();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .init();

    let convert = ConvertCommand::parse();

}
