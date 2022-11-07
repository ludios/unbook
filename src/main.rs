use clap::Parser;
use mimalloc::MiMalloc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser, Debug)]
#[clap(name = "unbook", version)]
/// unbook
enum UnbookCommand {
    /// Do something
    #[clap(name = "something")]
    Something {}
}

fn main() {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("warn"))
        .unwrap();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .init();

    let command = UnbookCommand::parse();
    match command {
        UnbookCommand::Something {} => {
            info!("something");
        }
    }
}
