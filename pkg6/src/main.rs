mod error;
use error::{Pkg6Error, Result};

use clap::{Parser, Subcommand};
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt};

/// Wrapper struct for publisher output in JSON format
#[derive(Serialize)]
struct PublishersOutput {
    publishers: Vec<PublisherOutput>,
}

/// Serializable struct for publisher information
#[derive(Serialize)]
struct PublisherOutput {
    name: String,
    origin: String,
    mirrors: Vec<String>,
    is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_dir: Option<String>,
}

/// pkg6 - Image Packaging System client
///
/// The pkg command is used to manage the software installed on an image.
/// An image can be a boot environment, a zone, or a non-global zone.
///
/// The pkg command manages the retrieval, installation, update, and removal
/// of software packages for the OpenIndiana operating system.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct App {
    /// Path to the image to operate on
    ///
    /// If not specified, the default image is determined as follows:
    /// - If $HOME/.pkg exists, that directory is used
    /// - Otherwise, the root directory (/) is used
    #[clap(short = 'R', global = true)]
    image_path: Option<PathBuf>,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Update the list of available packages and patches
    ///
    /// The refresh command updates the local package catalog, retrieving
    /// the latest list of available packages from the configured publishers.
    Refresh {
        /// Perform a full refresh, retrieving all package metadata
        #[clap(long)]
        full: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Publishers to refresh (default: all)
        publishers: Vec<String>,
    },

    /// Install or update packages
    ///
    /// The install command installs or updates packages from the configured
    /// publishers. If a package is already installed, it will be updated to
    /// the newest version available.
    Install {
        /// Dry run, don't make actual changes
        #[clap(short)]
        dry_run: bool,

        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Number of concurrent operations
        #[clap(short = 'C')]
        concurrency: Option<usize>,

        /// Additional package repository to use
        #[clap(short = 'g')]
        repo: Vec<String>,

        /// Accept all licenses
        #[clap(long)]
        accept: bool,

        /// Show all licenses
        #[clap(long)]
        licenses: bool,

        /// Don't update the search index
        #[clap(long)]
        no_index: bool,

        /// Don't refresh the catalog
        #[clap(long)]
        no_refresh: bool,

        /// Packages to install
        pkg_fmri_patterns: Vec<String>,
    },

    /// Install packages while removing all other packages
    ///
    /// The exact-install command installs the specified packages and removes
    /// all other packages. This is useful for creating a clean installation
    /// with only the specified packages.
    ExactInstall {
        /// Dry run, don't make actual changes
        #[clap(short)]
        dry_run: bool,

        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Number of concurrent operations
        #[clap(short = 'C')]
        concurrency: Option<usize>,

        /// Additional package repository to use
        #[clap(short = 'g')]
        repo: Vec<String>,

        /// Accept all licenses
        #[clap(long)]
        accept: bool,

        /// Show all licenses
        #[clap(long)]
        licenses: bool,

        /// Don't update the search index
        #[clap(long)]
        no_index: bool,

        /// Don't refresh the catalog
        #[clap(long)]
        no_refresh: bool,

        /// Packages to install
        pkg_fmri_patterns: Vec<String>,
    },

    /// Remove packages
    ///
    /// The uninstall command removes installed packages from the system.
    Uninstall {
        /// Dry run, don't make actual changes
        #[clap(short)]
        dry_run: bool,

        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Packages to remove
        pkg_fmri_patterns: Vec<String>,
    },

    /// Update packages to newer versions
    ///
    /// The update command updates installed packages to the newest versions
    /// available from the configured publishers.
    Update {
        /// Dry run, don't make actual changes
        #[clap(short)]
        dry_run: bool,

        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Number of concurrent operations
        #[clap(short = 'C')]
        concurrency: Option<usize>,

        /// Additional package repository to use
        #[clap(short = 'g')]
        repo: Vec<String>,

        /// Accept all licenses
        #[clap(long)]
        accept: bool,

        /// Show all licenses
        #[clap(long)]
        licenses: bool,

        /// Don't update the search index
        #[clap(long)]
        no_index: bool,

        /// Don't refresh the catalog
        #[clap(long)]
        no_refresh: bool,

        /// Packages to update (default: all)
        pkg_fmri_patterns: Vec<String>,
    },

    /// List installed packages
    ///
    /// The list command displays information about installed packages.
    /// By default, it lists only installed packages. Use the -a flag to list all available packages.
    List {
        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// List all available packages, not just installed ones
        #[clap(short)]
        all: bool,

        /// Output format (default: table)
        #[clap(short = 'o')]
        output_format: Option<String>,

        /// Packages to list (default: all)
        pkg_fmri_patterns: Vec<String>,
    },

    /// Display information about packages
    ///
    /// The info command displays detailed information about packages.
    Info {
        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Output format (default: table)
        #[clap(short = 'o')]
        output_format: Option<String>,

        /// Packages to show information about
        pkg_fmri_patterns: Vec<String>,
    },

    /// Search for packages
    ///
    /// The search command searches for packages matching the specified query.
    Search {
        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Output format (default: table)
        #[clap(short = 'o')]
        output_format: Option<String>,

        /// Search query
        query: String,
    },

    /// Verify installation of packages
    ///
    /// The verify command verifies that installed packages match their
    /// manifest and that all files are present and have the correct
    /// permissions and checksums.
    Verify {
        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Packages to verify (default: all)
        pkg_fmri_patterns: Vec<String>,
    },

    /// Fix package installation problems
    ///
    /// The fix command repairs packages with missing or corrupt files.
    Fix {
        /// Dry run, don't make actual changes
        #[clap(short)]
        dry_run: bool,

        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Packages to fix (default: all)
        pkg_fmri_patterns: Vec<String>,
    },

    /// Show history of package operations
    ///
    /// The history command displays the history of package operations.
    History {
        /// Number of entries to show
        #[clap(short = 'n')]
        count: Option<usize>,

        /// Show full details
        #[clap(short)]
        full: bool,

        /// Output format (default: table)
        #[clap(short = 'o')]
        output_format: Option<String>,
    },

    /// List contents of packages
    ///
    /// The contents command lists the contents of packages.
    Contents {
        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

        /// Output format (default: table)
        #[clap(short = 'o')]
        output_format: Option<String>,

        /// Packages to list contents of
        pkg_fmri_patterns: Vec<String>,
    },

    /// Set publisher properties
    ///
    /// The set-publisher command sets properties for publishers.
    SetPublisher {
        /// Publisher name
        #[clap(short = 'p')]
        publisher: String,

        /// Publisher origin URL
        #[clap(short = 'O')]
        origin: Option<String>,

        /// Publisher mirror URL
        #[clap(short = 'M')]
        mirror: Option<Vec<String>>,
    },

    /// Remove a publisher
    ///
    /// The unset-publisher command removes a publisher.
    UnsetPublisher {
        /// Publisher name
        publisher: String,
    },

    /// Display publisher information
    ///
    /// The publisher command displays information about publishers.
    Publisher {
        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Output format (default: table)
        #[clap(short = 'o')]
        output_format: Option<String>,

        /// Publishers to show information about (default: all)
        publishers: Vec<String>,
    },

    /// Create an image
    ///
    /// The image-create command creates a new image.
    /// If publisher and origin are provided, the publisher will be added to the image.
    ImageCreate {
        /// Full path to the image to create
        #[clap(short = 'F')]
        full_path: PathBuf,

        /// Publisher to use (optional)
        #[clap(short = 'p')]
        publisher: Option<String>,

        /// Publisher origin URL (required if publisher is specified)
        #[clap(short = 'g', requires = "publisher")]
        origin: Option<String>,

        /// Type of image to create (full or partial, default: full)
        #[clap(short = 't', long = "type", default_value = "full")]
        image_type: String,
    },

    /// Debug database commands (hidden)
    ///
    /// These commands are for debugging purposes only and are not part of the public API.
    /// They are used to inspect the contents of the redb databases for debugging purposes.
    ///
    /// Usage examples:
    /// - Show database statistics: pkg6 debug-db --stats
    /// - Dump all tables: pkg6 debug-db --dump-all
    /// - Dump a specific table: pkg6 debug-db --dump-table catalog
    ///
    /// Available tables:
    /// - catalog: Contains non-obsolete packages (in catalog.redb)
    /// - obsoleted: Contains obsolete packages (in catalog.redb)
    /// - installed: Contains installed packages (in installed.redb)
    #[clap(hide = true)]
    DebugDb {
        /// Show database statistics
        #[clap(long)]
        stats: bool,

        /// Dump all tables
        #[clap(long)]
        dump_all: bool,

        /// Dump a specific table (catalog, obsoleted, installed)
        #[clap(long)]
        dump_table: Option<String>,
    },
}

/// Determines the image path to use based on the provided argument and default rules
///
/// If the image_path argument is provided, that path is used.
/// Otherwise, if $HOME/.pkg exists, that path is used.
/// Otherwise, the root directory (/) is used.
fn determine_image_path(image_path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = image_path {
        // Use the explicitly provided path
        debug!("Using explicitly provided image path: {}", path.display());
        path
    } else {
        // Check if $HOME/.pkg exists
        if let Ok(home_dir) = std::env::var("HOME") {
            let home_pkg = PathBuf::from(home_dir).join(".pkg");
            if home_pkg.exists() {
                debug!("Using user home image path: {}", home_pkg.display());
                return PathBuf::from(home_pkg);
            }
        }

        // Default to root directory
        debug!("Using root directory as image path");
        PathBuf::from("/")
    }
}

fn main() -> Result<()> {
    // Initialize the tracing subscriber with the default log level as debug and no decorations
    // Parse the environment filter first, handling any errors with our custom error type
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .from_env()
        .map_err(|e| {
            Pkg6Error::LoggingEnvError(format!("Failed to parse environment filter: {}", e))
        })?;

    fmt::Subscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter(env_filter)
        .without_time()
        .with_target(false)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = App::parse();

    match &cli.command {
        Commands::Refresh {
            full,
            quiet,
            publishers,
        } => {
            info!("Refreshing package catalog");
            debug!("Full refresh: {}", full);
            debug!("Quiet mode: {}", quiet);
            debug!("Publishers: {:?}", publishers);

            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            if !quiet {
                println!("Using image at: {}", image_path.display());
            }

            // Try to load the image from the determined path
            let image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    if !quiet {
                        eprintln!("Failed to load image from {}: {}", image_path.display(), e);
                        eprintln!(
                            "Make sure the path points to a valid image or use pkg6 image-create first"
                        );
                    }
                    return Err(e.into());
                }
            };

            // Refresh the catalogs
            if let Err(e) = image.refresh_catalogs(publishers, *full) {
                error!("Failed to refresh catalog: {}", e);
                if !quiet {
                    eprintln!("Failed to refresh catalog: {}", e);
                }
                return Err(e.into());
            }

            info!("Refresh completed successfully");
            if !quiet {
                println!("Refresh completed successfully");
            }
            Ok(())
        }
        Commands::Install {
            dry_run,
            verbose,
            quiet,
            concurrency,
            repo,
            accept,
            licenses,
            no_index,
            no_refresh,
            pkg_fmri_patterns,
        } => {
            info!("Installing packages: {:?}", pkg_fmri_patterns);
            debug!("Dry run: {}", dry_run);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Concurrency: {:?}", concurrency);
            debug!("Additional repos: {:?}", repo);
            debug!("Accept licenses: {}", accept);
            debug!("Show licenses: {}", licenses);
            debug!("No index update: {}", no_index);
            debug!("No refresh: {}", no_refresh);

            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            if !quiet {
                println!("Using image at: {}", image_path.display());
            }

            // Load the image
            let image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    return Err(e.into());
                }
            };

            // Note: Install now relies on existing redb databases and does not perform
            // a full import or refresh automatically. Run `pkg6 refresh` explicitly
            // to update catalogs before installing if needed.
            if !*quiet {
                eprintln!(
                    "Install uses existing catalogs in redb; run 'pkg6 refresh' to update catalogs if needed."
                );
            }

            // Build solver constraints from the provided pkg specs
            if pkg_fmri_patterns.is_empty() {
                if !quiet {
                    eprintln!("No packages specified to install");
                }
                return Err(Pkg6Error::Other("no packages specified".to_string()));
            }
            let mut constraints: Vec<libips::solver::Constraint> = Vec::new();
            for spec in pkg_fmri_patterns {
                let mut preferred_publishers: Vec<String> = Vec::new();
                let mut name_part = spec.as_str();
                // parse optional publisher prefix pkg://<pub>/
                if let Some(rest) = name_part.strip_prefix("pkg://") {
                    if let Some((pubr, rest2)) = rest.split_once('/') {
                        preferred_publishers.push(pubr.to_string());
                        name_part = rest2;
                    }
                }
                // split version requirement after '@'
                let (stem, version_req) = if let Some((s, v)) = name_part.split_once('@') {
                    (s.to_string(), Some(v.to_string()))
                } else {
                    (name_part.to_string(), None)
                };
                constraints.push(libips::solver::Constraint {
                    stem,
                    version_req,
                    preferred_publishers,
                    branch: None,
                });
            }

            // Resolve install plan
            if !quiet {
                println!("Resolving dependencies...");
            }
            let plan = match libips::solver::resolve_install(&image, &constraints) {
                Ok(p) => p,
                Err(e) => {
                    let mut printed_advice = false;
                    if !*quiet {
                        // Attempt to provide user-focused advice on how to resolve dependency issues
                        let opts = libips::solver::advice::AdviceOptions {
                            max_depth: 3,
                            dependency_cap: 400,
                        };
                        match libips::solver::advice::advise_from_error(&image, &e, opts) {
                            Ok(report) => {
                                if !report.issues.is_empty() {
                                    printed_advice = true;
                                    eprintln!(
                                        "\nAdvice: detected {} issue(s) preventing installation:",
                                        report.issues.len()
                                    );
                                    for (i, iss) in report.issues.iter().enumerate() {
                                        let constraint_str = {
                                            let mut s = String::new();
                                            if let Some(r) = &iss.constraint_release {
                                                s.push_str(&format!("release={} ", r));
                                            }
                                            if let Some(b) = &iss.constraint_branch {
                                                s.push_str(&format!("branch={}", b));
                                            }
                                            s.trim().to_string()
                                        };
                                        eprintln!(
                                            "  {}. Missing viable candidates for '{}'\n     - Path: {}\n     - Constraint: {}\n     - Details: {}",
                                            i + 1,
                                            iss.stem,
                                            if iss.path.is_empty() {
                                                iss.stem.clone()
                                            } else {
                                                iss.path.join(" -> ")
                                            },
                                            if constraint_str.is_empty() {
                                                "<none>".to_string()
                                            } else {
                                                constraint_str
                                            },
                                            iss.details
                                        );
                                    }
                                    eprintln!("\nWhat you can try as a user:");
                                    eprintln!(
                                        "  • Ensure your catalogs are up to date: 'pkg6 refresh'."
                                    );
                                    eprintln!(
                                        "  • Verify that the required publishers are configured: 'pkg6 publisher'."
                                    );
                                    eprintln!(
                                        "  • Some versions may be constrained by image incorporations; updating the image or selecting a compatible package set may help."
                                    );
                                    eprintln!(
                                        "  • If the problem persists, report this to the repository maintainers with the above details."
                                    );
                                }
                            }
                            Err(advice_err) => {
                                eprintln!("(Note) Unable to compute advice: {}", advice_err);
                            }
                        }
                    }
                    if printed_advice {
                        // We've printed actionable advice; exit with a non-zero code without printing further errors.
                        std::process::exit(1);
                    } else {
                        // No advice printed; fall back to standard error reporting
                        error!("Failed to resolve install plan: {}", e);
                        return Err(e.into());
                    }
                }
            };

            if !quiet {
                println!("Resolved {} package(s) to install", plan.add.len());
            }

            // Build and apply action plan
            if !quiet {
                println!("Building action plan...");
            }
            let ap = libips::image::action_plan::ActionPlan::from_install_plan(&plan);
            let quiet_mode = *quiet;
            let progress_cb: libips::actions::executors::ProgressCallback = Arc::new(move |evt| {
                if quiet_mode {
                    return;
                }
                match evt {
                    libips::actions::executors::ProgressEvent::StartingPhase { phase, total } => {
                        println!("Applying: {} (total {})...", phase, total);
                    }
                    libips::actions::executors::ProgressEvent::Progress {
                        phase,
                        current,
                        total,
                    } => {
                        println!("Applying: {} {}/{}", phase, current, total);
                    }
                    libips::actions::executors::ProgressEvent::FinishedPhase { phase, total } => {
                        println!("Done: {} (total {})", phase, total);
                    }
                }
            });
            let apply_opts = libips::actions::executors::ApplyOptions {
                dry_run: *dry_run,
                progress: Some(progress_cb),
                progress_interval: 10,
            };
            if !quiet {
                println!("Applying action plan (dry-run: {})", dry_run);
            }
            ap.apply(image.path(), &apply_opts)?;

            // Update installed DB after success (skip on dry-run)
            if !*dry_run {
                if !quiet {
                    println!("Recording installation in image database...");
                }
                let total_pkgs = plan.add.len();
                let mut idx = 0usize;
                for rp in &plan.add {
                    image.install_package(&rp.fmri, &rp.manifest)?;
                    idx += 1;
                    if !quiet && (idx % 5 == 0 || idx == total_pkgs) {
                        println!("Recorded {}/{} packages", idx, total_pkgs);
                    }
                    // Save full manifest into manifests directory for reproducibility
                    match image.save_manifest(&rp.fmri, &rp.manifest) {
                        Ok(path) => {
                            if *verbose && !*quiet {
                                eprintln!("Saved manifest for {} to {}", rp.fmri, path.display());
                            }
                        }
                        Err(e) => {
                            // Non-fatal: log error but continue install
                            error!("Failed to save manifest for {}: {}", rp.fmri, e);
                        }
                    }
                }
                if !quiet {
                    println!("Installed {} package(s)", plan.add.len());
                }

                // Dump installed database to make changes visible
                let installed =
                    libips::image::installed::InstalledPackages::new(image.installed_db_path());
                if let Err(e) = installed.dump_installed_table() {
                    error!("Failed to dump installed database: {}", e);
                }
            } else if !quiet {
                println!(
                    "Dry-run completed: {} package(s) would be installed",
                    plan.add.len()
                );
            }

            info!("Installation completed successfully");
            Ok(())
        }
        Commands::ExactInstall {
            dry_run,
            verbose,
            quiet,
            concurrency,
            repo,
            accept,
            licenses,
            no_index,
            no_refresh,
            pkg_fmri_patterns,
        } => {
            info!("Exact-installing packages: {:?}", pkg_fmri_patterns);
            debug!("Dry run: {}", dry_run);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Concurrency: {:?}", concurrency);
            debug!("Additional repos: {:?}", repo);
            debug!("Accept licenses: {}", accept);
            debug!("Show licenses: {}", licenses);
            debug!("No index update: {}", no_index);
            debug!("No refresh: {}", no_refresh);

            // Stub implementation
            info!("Exact-installation completed successfully");
            Ok(())
        }
        Commands::Uninstall {
            dry_run,
            verbose,
            quiet,
            pkg_fmri_patterns,
        } => {
            info!("Uninstalling packages: {:?}", pkg_fmri_patterns);
            debug!("Dry run: {}", dry_run);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);

            // Stub implementation
            info!("Uninstallation completed successfully");
            Ok(())
        }
        Commands::Update {
            dry_run,
            verbose,
            quiet,
            concurrency,
            repo,
            accept,
            licenses,
            no_index,
            no_refresh,
            pkg_fmri_patterns,
        } => {
            info!("Updating packages: {:?}", pkg_fmri_patterns);
            debug!("Dry run: {}", dry_run);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Concurrency: {:?}", concurrency);
            debug!("Additional repos: {:?}", repo);
            debug!("Accept licenses: {}", accept);
            debug!("Show licenses: {}", licenses);
            debug!("No index update: {}", no_index);
            debug!("No refresh: {}", no_refresh);

            // Stub implementation
            info!("Update completed successfully");
            Ok(())
        }
        Commands::List {
            verbose,
            quiet,
            all,
            output_format,
            pkg_fmri_patterns,
        } => {
            info!("Listing packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("All packages: {}", all);
            debug!("Output format: {:?}", output_format);

            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            info!("Using image at: {}", image_path.display());

            // Try to load the image from the determined path
            let image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    error!(
                        "Make sure the path points to a valid image or use pkg6 image-create first"
                    );
                    return Err(e.into());
                }
            };

            // Convert pkg_fmri_patterns to a single pattern if provided
            let pattern = if pkg_fmri_patterns.is_empty() {
                None
            } else {
                // For simplicity, we'll just use the first pattern
                // In a more complete implementation, we would handle multiple patterns
                Some(pkg_fmri_patterns[0].as_str())
            };

            if *all {
                // List all available packages
                info!("Listing all available packages");

                // Build the catalog before querying it
                info!("Building catalog...");
                if let Err(e) = image.build_catalog() {
                    error!("Failed to build catalog: {}", e);
                    return Err(e.into());
                }

                match image.query_catalog(pattern) {
                    Ok(packages) => {
                        println!(
                            "PUBLISHER                                  NAME                                     VERSION                      STATE"
                        );
                        println!(
                            "------------------------------------------------------------------------------------------------------------------------------------------------------"
                        );
                        for pkg in packages {
                            let state = if image.is_package_installed(&pkg.fmri).unwrap_or(false) {
                                "installed"
                            } else {
                                "known"
                            };
                            println!(
                                "{:<40} {:<40} {:<30} {}",
                                pkg.fmri.publisher.as_deref().unwrap_or("unknown"),
                                pkg.fmri.name,
                                pkg.fmri.version(),
                                state
                            );
                        }
                    }
                    Err(e) => {
                        error!("Failed to query catalog: {}", e);
                        return Err(e.into());
                    }
                }
            } else {
                // List only installed packages
                info!("Listing installed packages");
                match image.query_installed_packages(pattern) {
                    Ok(packages) => {
                        println!(
                            "PUBLISHER                                  NAME                                     VERSION                      STATE"
                        );
                        println!(
                            "------------------------------------------------------------------------------------------------------------------------------------------------------"
                        );
                        for pkg in packages {
                            println!(
                                "{:<40} {:<40} {:<30} {}",
                                pkg.fmri.publisher.as_deref().unwrap_or("unknown"),
                                pkg.fmri.name,
                                pkg.fmri.version(),
                                "installed"
                            );
                        }
                    }
                    Err(e) => {
                        error!("Failed to query installed packages: {}", e);
                        return Err(e.into());
                    }
                }
            }

            info!("List completed successfully");
            Ok(())
        }
        Commands::Info {
            verbose,
            quiet,
            output_format,
            pkg_fmri_patterns,
        } => {
            info!("Showing info for packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Output format: {:?}", output_format);

            // Stub implementation
            info!("Info completed successfully");
            Ok(())
        }
        Commands::Search {
            verbose,
            quiet,
            output_format,
            query,
        } => {
            info!("Searching for packages matching: {}", query);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Output format: {:?}", output_format);

            // Stub implementation
            info!("Search completed successfully");
            Ok(())
        }
        Commands::Verify {
            verbose,
            quiet,
            pkg_fmri_patterns,
        } => {
            info!("Verifying packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);

            // Stub implementation
            info!("Verification completed successfully");
            Ok(())
        }
        Commands::Fix {
            dry_run,
            verbose,
            quiet,
            pkg_fmri_patterns,
        } => {
            info!("Fixing packages: {:?}", pkg_fmri_patterns);
            debug!("Dry run: {}", dry_run);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);

            // Stub implementation
            info!("Fix completed successfully");
            Ok(())
        }
        Commands::History {
            count,
            full,
            output_format,
        } => {
            info!("Showing history");
            debug!("Count: {:?}", count);
            debug!("Full: {}", full);
            debug!("Output format: {:?}", output_format);

            // Stub implementation
            info!("History completed successfully");
            Ok(())
        }
        Commands::Contents {
            verbose,
            quiet,
            output_format,
            pkg_fmri_patterns,
        } => {
            info!("Showing contents for packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Output format: {:?}", output_format);

            // Stub implementation
            info!("Contents completed successfully");
            Ok(())
        }
        Commands::SetPublisher {
            publisher,
            origin,
            mirror,
        } => {
            info!("Setting publisher: {}", publisher);
            debug!("Origin: {:?}", origin);
            debug!("Mirror: {:?}", mirror);

            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            info!("Using image at: {}", image_path.display());

            // Try to load the image from the determined path
            let mut image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    error!(
                        "Make sure the path points to a valid image or use pkg6 image-create first"
                    );
                    return Err(e.into());
                }
            };

            // Convert mirror to Vec<String> if provided
            let mirrors = match mirror {
                Some(m) => m.clone(),
                None => vec![],
            };

            // If origin is provided, update the publisher
            if let Some(origin_url) = origin {
                // Add or update the publisher
                image.add_publisher(&publisher, &origin_url, mirrors, true)?;
                info!(
                    "Publisher {} configured with origin: {}",
                    publisher, origin_url
                );

                // Download the catalog
                image.download_publisher_catalog(&publisher)?;
                info!("Catalog downloaded from publisher: {}", publisher);
            } else {
                // If no origin is provided, just set the publisher as default if it exists
                let pub_result = image.get_publisher(&publisher);
                if let Ok(pub_info) = pub_result {
                    // Store the necessary information
                    let origin = pub_info.origin.clone();
                    let mirrors = pub_info.mirrors.clone();

                    // Add the publisher again with is_default=true to make it the default
                    image.add_publisher(&publisher, &origin, mirrors, true)?;
                    info!("Publisher {} set as default", publisher);
                } else {
                    error!("Publisher {} not found and no origin provided", publisher);
                    return Err(
                        libips::image::ImageError::PublisherNotFound(publisher.clone()).into(),
                    );
                }
            }

            info!("Set-publisher completed successfully");
            Ok(())
        }
        Commands::UnsetPublisher { publisher } => {
            info!("Unsetting publisher: {}", publisher);

            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            info!("Using image at: {}", image_path.display());

            // Try to load the image from the determined path
            let mut image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    error!(
                        "Make sure the path points to a valid image or use pkg6 image-create first"
                    );
                    return Err(e.into());
                }
            };

            // Remove the publisher
            image.remove_publisher(&publisher)?;

            // Refresh the catalog to reflect the current state of all available packages
            if let Err(e) = image.download_catalogs() {
                error!("Failed to refresh catalog after removing publisher: {}", e);
                // Continue execution even if catalog refresh fails
            } else {
                info!("Catalog refreshed successfully");
            }

            info!("Publisher {} removed successfully", publisher);
            info!("Unset-publisher completed successfully");
            Ok(())
        }
        Commands::Publisher {
            verbose,
            output_format,
            publishers,
        } => {
            info!("Showing publisher information");

            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            info!("Using image at: {}", image_path.display());

            // Try to load the image from the determined path
            let image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    error!(
                        "Make sure the path points to a valid image or use pkg6 image-create first"
                    );
                    return Err(e.into());
                }
            };

            // Get all publishers
            let all_publishers = image.publishers();

            // Filter publishers if specified
            let filtered_publishers: Vec<_> = if publishers.is_empty() {
                all_publishers.to_vec()
            } else {
                all_publishers
                    .iter()
                    .filter(|p| publishers.contains(&p.name))
                    .cloned()
                    .collect()
            };

            // Handle case where no publishers are found
            if filtered_publishers.is_empty() {
                if publishers.is_empty() {
                    println!("No publishers configured");
                } else {
                    println!("No matching publishers found");
                }
                return Ok(());
            }

            // Determine the output format, defaulting to "table" if not specified
            let output_format_str = output_format.as_deref().unwrap_or("table");

            // Create a vector of PublisherOutput structs for serialization and display
            let publisher_outputs: Vec<PublisherOutput> = filtered_publishers
                .iter()
                .map(|p| {
                    let catalog_dir = if *verbose {
                        let dir = match image.image_type() {
                            libips::image::ImageType::Full => image_path.join("var/pkg/catalog"),
                            libips::image::ImageType::Partial => image_path.join(".pkg/catalog"),
                        };
                        Some(dir.join(&p.name).display().to_string())
                    } else {
                        None
                    };

                    PublisherOutput {
                        name: p.name.clone(),
                        origin: p.origin.clone(),
                        mirrors: p.mirrors.clone(),
                        is_default: p.is_default,
                        catalog_dir,
                    }
                })
                .collect();

            // Display publisher information based on the output format
            match output_format_str {
                "table" => {
                    // Display in table format (human-readable)
                    // This is the default format and displays the information in a user-friendly way
                    for publisher in &publisher_outputs {
                        println!("Publisher: {}", publisher.name);
                        println!("  Origin: {}", publisher.origin);
                        if !publisher.mirrors.is_empty() {
                            println!("  Mirrors:");
                            for mirror in &publisher.mirrors {
                                println!("    {}", mirror);
                            }
                        }
                        println!(
                            "  Default: {}",
                            if publisher.is_default { "Yes" } else { "No" }
                        );
                        if let Some(catalog_dir) = &publisher.catalog_dir {
                            println!("  Catalog directory: {}", catalog_dir);
                        }
                        println!();
                        // Explicitly flush stdout after each publisher to ensure output is displayed
                        let _ = std::io::stdout().flush();
                    }
                }
                "json" => {
                    // Display in JSON format
                    // This format is useful for programmatic access to the publisher information
                    let output = PublishersOutput {
                        publishers: publisher_outputs,
                    };
                    let json = serde_json::to_string_pretty(&output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                    println!("{}", json);
                    let _ = std::io::stdout().flush();
                }
                "tsv" => {
                    // Display in TSV format (tab-separated values)
                    // This format is useful for importing into spreadsheets or other data processing tools
                    // Print header
                    println!("NAME\tORIGIN\tMIRRORS\tDEFAULT\tCATALOG_DIR");

                    // Print each publisher
                    for publisher in &publisher_outputs {
                        let mirrors = publisher.mirrors.join(",");
                        let default = if publisher.is_default { "Yes" } else { "No" };
                        let catalog_dir = publisher.catalog_dir.as_deref().unwrap_or("");

                        println!(
                            "{}\t{}\t{}\t{}\t{}",
                            publisher.name, publisher.origin, mirrors, default, catalog_dir
                        );
                        let _ = std::io::stdout().flush();
                    }
                }
                _ => {
                    // Unsupported format
                    return Err(Pkg6Error::UnsupportedOutputFormat(
                        output_format_str.to_string(),
                    ));
                }
            }

            info!("Publisher completed successfully");
            Ok(())
        }
        Commands::ImageCreate {
            full_path,
            publisher,
            origin,
            image_type,
        } => {
            info!("Creating image at: {}", full_path.display());
            debug!("Publisher: {:?}", publisher);
            debug!("Origin: {:?}", origin);
            debug!("Image type: {}", image_type);

            // Convert the image type string to the ImageType enum
            let image_type = match image_type.to_lowercase().as_str() {
                "full" => libips::image::ImageType::Full,
                "partial" => libips::image::ImageType::Partial,
                _ => {
                    error!("Invalid image type: {}. Using default (full)", image_type);
                    libips::image::ImageType::Full
                }
            };

            // Create the image (only creates the basic structure)
            let mut image = libips::image::Image::create_image(&full_path, image_type)?;
            info!("Image created successfully at: {}", full_path.display());

            // If publisher and origin are provided, only add the publisher; do not download/open catalogs here.
            if let (Some(publisher_name), Some(origin_url)) = (publisher.as_ref(), origin.as_ref())
            {
                info!(
                    "Adding publisher {} with origin {}",
                    publisher_name, origin_url
                );

                // Add the publisher
                image.add_publisher(publisher_name, origin_url, vec![], true)?;

                info!(
                    "Publisher {} configured with origin: {}",
                    publisher_name, origin_url
                );
                info!(
                    "Catalogs are not downloaded during image creation. Use 'pkg6 -R {} refresh {}' to download and open catalogs.",
                    full_path.display(),
                    publisher_name
                );
            } else {
                info!("No publisher configured. Use 'pkg6 set-publisher' to add a publisher.");
            }

            Ok(())
        }
        Commands::DebugDb {
            stats,
            dump_all,
            dump_table,
        } => {
            info!("Debug database command");
            debug!("Stats: {}", stats);
            debug!("Dump all: {}", dump_all);
            debug!("Dump table: {:?}", dump_table);

            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            info!("Using image at: {}", image_path.display());

            // Try to load the image from the determined path
            let image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    error!(
                        "Make sure the path points to a valid image or use pkg6 image-create first"
                    );
                    return Err(e.into());
                }
            };

            // Create a catalog object for the catalog.redb database
            let catalog = libips::image::catalog::ImageCatalog::new(
                image.catalog_dir(),
                image.active_db_path(),
                image.obsolete_db_path(),
            );

            // Create an installed packages object for the installed.redb database
            let installed =
                libips::image::installed::InstalledPackages::new(image.installed_db_path());

            // Execute the requested debug command
            if *stats {
                info!("Showing database statistics");
                println!("=== CATALOG DATABASE ===");
                if let Err(e) = catalog.get_db_stats() {
                    error!("Failed to get catalog database statistics: {}", e);
                    return Err(Pkg6Error::Other(format!(
                        "Failed to get catalog database statistics: {}",
                        e
                    )));
                }

                println!("\n=== INSTALLED DATABASE ===");
                if let Err(e) = installed.get_db_stats() {
                    error!("Failed to get installed database statistics: {}", e);
                    return Err(Pkg6Error::Other(format!(
                        "Failed to get installed database statistics: {}",
                        e
                    )));
                }
            }

            if *dump_all {
                info!("Dumping all tables");
                println!("=== CATALOG DATABASE ===");
                if let Err(e) = catalog.dump_all_tables() {
                    error!("Failed to dump catalog database tables: {}", e);
                    return Err(Pkg6Error::Other(format!(
                        "Failed to dump catalog database tables: {}",
                        e
                    )));
                }

                println!("\n=== INSTALLED DATABASE ===");
                if let Err(e) = installed.dump_installed_table() {
                    error!("Failed to dump installed database table: {}", e);
                    return Err(Pkg6Error::Other(format!(
                        "Failed to dump installed database table: {}",
                        e
                    )));
                }
            }

            if let Some(table_name) = dump_table {
                info!("Dumping table: {}", table_name);

                // Determine which database to use based on the table name
                match table_name.as_str() {
                    "installed" => {
                        // Use the installed packages database
                        println!("=== INSTALLED DATABASE ===");
                        if let Err(e) = installed.dump_installed_table() {
                            error!("Failed to dump installed table: {}", e);
                            return Err(Pkg6Error::Other(format!(
                                "Failed to dump installed table: {}",
                                e
                            )));
                        }
                    }
                    "catalog" | "obsoleted" => {
                        // Use the catalog database
                        println!("=== CATALOG DATABASE ===");
                        if let Err(e) = catalog.dump_table(table_name) {
                            error!("Failed to dump table {}: {}", table_name, e);
                            return Err(Pkg6Error::Other(format!(
                                "Failed to dump table {}: {}",
                                table_name, e
                            )));
                        }
                    }
                    _ => {
                        error!("Unknown table: {}", table_name);
                        return Err(Pkg6Error::Other(format!(
                            "Unknown table: {}. Available tables: catalog, obsoleted, installed",
                            table_name
                        )));
                    }
                }
            }

            info!("Debug database command completed successfully");
            Ok(())
        }
    }
}
