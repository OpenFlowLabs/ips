use clap::Parser;
use libips::fmri::Fmri;
use libips::recv::PackageReceiver;
use libips::repository::{
    FileBackend, ProgressInfo, ProgressReporter, ReadableRepository, RestBackend,
};
use miette::{IntoDiagnostic, Result};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

struct ConsoleProgressReporter;

impl ProgressReporter for ConsoleProgressReporter {
    fn start(&self, info: &ProgressInfo) {
        info!("{}", info);
    }
    fn update(&self, info: &ProgressInfo) {
        info!("{}", info);
    }
    fn finish(&self, info: &ProgressInfo) {
        info!("DONE: {}", info.operation);
    }
}

#[derive(Parser)]
#[command(name = "pkg6recv")]
#[command(about = "Receive packages from a repository", long_about = None)]
struct Cli {
    /// Source repository URI or path
    #[arg(short = 's', long)]
    source: String,

    /// Destination repository path
    #[arg(short = 'd', long)]
    dest: PathBuf,

    /// Packages to receive (FMRIs)
    packages: Vec<String>,

    /// Receive dependencies recursively
    #[arg(short = 'r', long)]
    recursive: bool,

    /// Default publisher name if not specified in FMRI
    #[arg(short = 'p', long)]
    publisher: Option<String>,
}

fn main() -> Result<()> {
    // Initialize tracing
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    // Open destination repository
    // We'll open it inside each branch to avoid borrow checker issues with moves

    let fmris: Vec<Fmri> = cli
        .packages
        .iter()
        .map(|s| Fmri::parse(s))
        .collect::<std::result::Result<Vec<_>, _>>()
        .into_diagnostic()?;

    let progress = ConsoleProgressReporter;

    // Determine if source is a URL or a path and receive packages
    if cli.source.starts_with("http://") || cli.source.starts_with("https://") {
        let mut source_repo = RestBackend::open(&cli.source).into_diagnostic()?;
        let dest_repo = FileBackend::open(&cli.dest).into_diagnostic()?;
        let mut receiver = PackageReceiver::new(&mut source_repo, dest_repo);
        receiver = receiver.with_progress(&progress);
        receiver
            .receive(cli.publisher.as_deref(), &fmris, cli.recursive)
            .into_diagnostic()?;
    } else {
        let mut source_repo = FileBackend::open(&cli.source).into_diagnostic()?;
        let dest_repo = FileBackend::open(&cli.dest).into_diagnostic()?;
        let mut receiver = PackageReceiver::new(&mut source_repo, dest_repo);
        receiver = receiver.with_progress(&progress);
        receiver
            .receive(cli.publisher.as_deref(), &fmris, cli.recursive)
            .into_diagnostic()?;
    }

    info!("Package receive complete.");

    Ok(())
}
