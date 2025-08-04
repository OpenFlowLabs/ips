mod error;
use error::{Pkg6Error, Result};

use clap::{Parser, Subcommand};
use serde::Serialize;
use std::path::PathBuf;
use std::io::Write;
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
    List {
        /// Verbose output
        #[clap(short)]
        verbose: bool,

        /// Quiet mode, show less output
        #[clap(short)]
        quiet: bool,

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
    // Add debug statement at the very beginning
    eprintln!("MAIN: Starting pkg6 command");
    
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

    eprintln!("MAIN: Parsing command line arguments");
    let cli = App::parse();
    
    // Print the command that was parsed
    match &cli.command {
        Commands::Publisher { .. } => eprintln!("MAIN: Publisher command detected"),
        _ => eprintln!("MAIN: Other command detected: {:?}", cli.command),
    };

    match &cli.command {
        Commands::Refresh { full, quiet, publishers } => {
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
                        eprintln!("Make sure the path points to a valid image or use pkg6 image-create first");
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
        },
        Commands::Install { dry_run, verbose, quiet, concurrency, repo, accept, licenses, no_index, no_refresh, pkg_fmri_patterns } => {
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
            
            // Stub implementation
            info!("Installation completed successfully");
            Ok(())
        },
        Commands::ExactInstall { dry_run, verbose, quiet, concurrency, repo, accept, licenses, no_index, no_refresh, pkg_fmri_patterns } => {
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
        },
        Commands::Uninstall { dry_run, verbose, quiet, pkg_fmri_patterns } => {
            info!("Uninstalling packages: {:?}", pkg_fmri_patterns);
            debug!("Dry run: {}", dry_run);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            
            // Stub implementation
            info!("Uninstallation completed successfully");
            Ok(())
        },
        Commands::Update { dry_run, verbose, quiet, concurrency, repo, accept, licenses, no_index, no_refresh, pkg_fmri_patterns } => {
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
        },
        Commands::List { verbose, quiet, output_format, pkg_fmri_patterns } => {
            info!("Listing packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Output format: {:?}", output_format);
            
            // Stub implementation
            info!("List completed successfully");
            Ok(())
        },
        Commands::Info { verbose, quiet, output_format, pkg_fmri_patterns } => {
            info!("Showing info for packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Output format: {:?}", output_format);
            
            // Stub implementation
            info!("Info completed successfully");
            Ok(())
        },
        Commands::Search { verbose, quiet, output_format, query } => {
            info!("Searching for packages matching: {}", query);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Output format: {:?}", output_format);
            
            // Stub implementation
            info!("Search completed successfully");
            Ok(())
        },
        Commands::Verify { verbose, quiet, pkg_fmri_patterns } => {
            info!("Verifying packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            
            // Stub implementation
            info!("Verification completed successfully");
            Ok(())
        },
        Commands::Fix { dry_run, verbose, quiet, pkg_fmri_patterns } => {
            info!("Fixing packages: {:?}", pkg_fmri_patterns);
            debug!("Dry run: {}", dry_run);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            
            // Stub implementation
            info!("Fix completed successfully");
            Ok(())
        },
        Commands::History { count, full, output_format } => {
            info!("Showing history");
            debug!("Count: {:?}", count);
            debug!("Full: {}", full);
            debug!("Output format: {:?}", output_format);
            
            // Stub implementation
            info!("History completed successfully");
            Ok(())
        },
        Commands::Contents { verbose, quiet, output_format, pkg_fmri_patterns } => {
            info!("Showing contents for packages: {:?}", pkg_fmri_patterns);
            debug!("Verbose: {}", verbose);
            debug!("Quiet: {}", quiet);
            debug!("Output format: {:?}", output_format);
            
            // Stub implementation
            info!("Contents completed successfully");
            Ok(())
        },
        Commands::SetPublisher { publisher, origin, mirror } => {
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
                    error!("Make sure the path points to a valid image or use pkg6 image-create first");
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
                info!("Publisher {} configured with origin: {}", publisher, origin_url);
                
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
                    return Err(libips::image::ImageError::PublisherNotFound(publisher.clone()).into());
                }
            }
            
            info!("Set-publisher completed successfully");
            Ok(())
        },
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
                    error!("Make sure the path points to a valid image or use pkg6 image-create first");
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
        },
        Commands::Publisher { verbose, output_format, publishers } => {
            info!("Showing publisher information");
            
            // Determine the image path using the -R argument or default rules
            let image_path = determine_image_path(cli.image_path.clone());
            info!("Using image at: {}", image_path.display());
            
            // Try to load the image from the determined path
            let image = match libips::image::Image::load(&image_path) {
                Ok(img) => img,
                Err(e) => {
                    error!("Failed to load image from {}: {}", image_path.display(), e);
                    error!("Make sure the path points to a valid image or use pkg6 image-create first");
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
                        println!("  Default: {}", if publisher.is_default { "Yes" } else { "No" });
                        if let Some(catalog_dir) = &publisher.catalog_dir {
                            println!("  Catalog directory: {}", catalog_dir);
                        }
                        println!();
                        // Explicitly flush stdout after each publisher to ensure output is displayed
                        let _ = std::io::stdout().flush();
                    }
                },
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
                },
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
                        
                        println!("{}\t{}\t{}\t{}\t{}", 
                            publisher.name, 
                            publisher.origin, 
                            mirrors, 
                            default, 
                            catalog_dir
                        );
                        let _ = std::io::stdout().flush();
                    }
                },
                _ => {
                    // Unsupported format
                    return Err(Pkg6Error::UnsupportedOutputFormat(output_format_str.to_string()));
                }
            }
            
            info!("Publisher completed successfully");
            Ok(())
        },
        Commands::ImageCreate { full_path, publisher, origin, image_type } => {
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
            
            // If publisher and origin are provided, add the publisher and download the catalog
            if let (Some(publisher_name), Some(origin_url)) = (publisher.as_ref(), origin.as_ref()) {
                info!("Adding publisher {} with origin {}", publisher_name, origin_url);
                
                // Add the publisher
                image.add_publisher(publisher_name, origin_url, vec![], true)?;
                
                // Download the catalog
                image.download_publisher_catalog(publisher_name)?;
                
                info!("Publisher {} configured with origin: {}", publisher_name, origin_url);
                info!("Catalog downloaded from publisher: {}", publisher_name);
            } else {
                info!("No publisher configured. Use 'pkg6 set-publisher' to add a publisher.");
            }
            
            Ok(())
        },
    }
}
