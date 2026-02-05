use clap::Parser;
use libips::fmri::Fmri;
use libips::recv::PackageReceiver;
use libips::repository::{
    FileBackend, ProgressInfo, ProgressReporter, ReadableRepository, RestBackend,
};
use miette::{IntoDiagnostic, Result};
use std::path::PathBuf;
use tracing::{info, warn};
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
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let progress = ConsoleProgressReporter;

    // Determine if source is a URL or a path and receive packages
    if cli.source.starts_with("http://") || cli.source.starts_with("https://") {
        let source_repo = RestBackend::open(&cli.source).into_diagnostic()?;
        let dest_repo = FileBackend::open(&cli.dest).into_diagnostic()?;
        
        let fmris = resolve_packages(&source_repo, cli.publisher.as_deref(), &cli.packages)?;
        
        let mut receiver = PackageReceiver::new(&source_repo, dest_repo);
        receiver = receiver.with_progress(&progress);
        receiver
            .receive(cli.publisher.as_deref(), &fmris, cli.recursive)
            .into_diagnostic()?;
    } else {
        let source_repo = FileBackend::open(&cli.source).into_diagnostic()?;
        let dest_repo = FileBackend::open(&cli.dest).into_diagnostic()?;
        
        let fmris = resolve_packages(&source_repo, cli.publisher.as_deref(), &cli.packages)?;
        
        let mut receiver = PackageReceiver::new(&source_repo, dest_repo);
        receiver = receiver.with_progress(&progress);
        receiver
            .receive(cli.publisher.as_deref(), &fmris, cli.recursive)
            .into_diagnostic()?;
    }

    info!("Package receive complete.");

    Ok(())
}

fn resolve_packages<R: ReadableRepository>(
    repo: &R,
    default_publisher: Option<&str>,
    packages: &[String],
) -> Result<Vec<Fmri>> {
    let mut resolved_fmris = Vec::new();

    for pkg_str in packages {
        if pkg_str.contains('*') || pkg_str.contains('?') {
            // It's a pattern, resolve it
            info!("Resolving wildcard pattern: {}", pkg_str);
            let matched = repo.list_packages(default_publisher, Some(pkg_str)).into_diagnostic()?;
            
            if matched.is_empty() {
                warn!("No packages matched pattern: {}", pkg_str);
            }
            
            // For each matched stem, we probably want the newest version if not specified.
            // list_packages returns all versions. PackageReceiver::receive also handles 
            // FMRIs without versions by picking the newest.
            // But list_packages returns full FMRIs. If the pattern matched multiple packages,
            // we get all versions of all of them.
            
            // To be consistent with IPS, if someone says "text/*", they usually want 
            // the latest version of everything that matches.
            
            let mut latest_versions: std::collections::HashMap<String, Fmri> = std::collections::HashMap::new();
            
            for pi in matched {
                let entry = latest_versions.entry(pi.fmri.name.clone());
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut oe) => {
                        if pi.fmri.version() > oe.get().version() {
                            oe.insert(pi.fmri);
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(ve) => {
                        ve.insert(pi.fmri);
                    }
                }
            }
            
            for (_, fmri) in latest_versions {
                info!("Found package: {}", fmri);
                resolved_fmris.push(fmri);
            }
        } else {
            // It's a regular FMRI or package name
            let fmri = Fmri::parse(pkg_str).into_diagnostic()?;
            resolved_fmris.push(fmri);
        }
    }

    Ok(resolved_fmris)
}
