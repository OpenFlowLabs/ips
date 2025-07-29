mod error;
mod pkg5_import;
use error::{Pkg6RepoError, Result};
use pkg5_import::Pkg5Importer;

/// URL encode a string for use in a filename
fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push_str(&format!("{:02X}", c as u8));
            }
        }
    }
    result
}

use clap::{Parser, Subcommand};
use libips::repository::{FileBackend, ReadableRepository, RepositoryVersion, WritableRepository};
use serde::Serialize;
use std::convert::TryFrom;
use std::path::PathBuf;
use tracing::{debug, info};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt};

#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod tests;

// Wrapper structs for JSON serialization
#[derive(Serialize)]
struct PropertiesOutput {
    #[serde(flatten)]
    properties: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
struct InfoOutput {
    publishers: Vec<libips::repository::PublisherInfo>,
}

#[derive(Serialize)]
struct PackagesOutput {
    packages: Vec<libips::repository::PackageInfo>,
}

#[derive(Serialize)]
struct SearchOutput {
    query: String,
    results: Vec<libips::repository::PackageInfo>,
}

#[derive(Serialize)]
struct ObsoletedPackagesOutput {
    packages: Vec<String>,
}

#[derive(Serialize)]
struct ObsoletedPackageDetailsOutput {
    fmri: String,
    status: String,
    obsolescence_date: String,
    deprecation_message: Option<String>,
    obsoleted_by: Option<Vec<String>>,
    metadata_version: u32,
    content_hash: String,
}

/// pkg6repo - Image Packaging System repository management utility
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct App {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new package repository
    Create {
        /// Version of the repository to create
        #[clap(long = "repo-version", default_value = "4")]
        repo_version: u32,

        /// Path or URI of the repository to create
        uri_or_path: String,
    },

    /// Add publishers to a repository
    AddPublisher {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Publishers to add
        publisher: Vec<String>,
    },

    /// Remove publishers from a repository
    RemovePublisher {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Perform a dry run
        #[clap(short = 'n')]
        dry_run: bool,

        /// Publishers to remove
        publisher: Vec<String>,
    },

    /// Get repository properties
    Get {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Output format
        #[clap(short = 'F')]
        format: Option<String>,

        /// Omit headers
        #[clap(short = 'H')]
        omit_headers: bool,

        /// Publisher to get properties for
        #[clap(short = 'p')]
        publisher: Option<Vec<String>>,

        /// SSL key file
        #[clap(long)]
        key: Option<PathBuf>,

        /// SSL certificate file
        #[clap(long)]
        cert: Option<PathBuf>,

        /// Properties to get (section/property)
        section_property: Option<Vec<String>>,
    },

    /// Display repository information
    Info {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Output format
        #[clap(short = 'F')]
        format: Option<String>,

        /// Omit headers
        #[clap(short = 'H')]
        omit_headers: bool,

        /// Publisher to get information for
        #[clap(short = 'p')]
        publisher: Option<Vec<String>>,

        /// SSL key file
        #[clap(long)]
        key: Option<PathBuf>,

        /// SSL certificate file
        #[clap(long)]
        cert: Option<PathBuf>,
    },

    /// List packages in a repository
    List {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Output format
        #[clap(short = 'F')]
        format: Option<String>,

        /// Omit headers
        #[clap(short = 'H')]
        omit_headers: bool,

        /// Publisher to list packages for
        #[clap(short = 'p')]
        publisher: Option<Vec<String>>,

        /// SSL key file
        #[clap(long)]
        key: Option<PathBuf>,

        /// SSL certificate file
        #[clap(long)]
        cert: Option<PathBuf>,

        /// Package FMRI patterns to match
        pkg_fmri_pattern: Option<Vec<String>>,
    },

    /// Show contents of packages in a repository
    Contents {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Show manifest contents
        #[clap(short = 'm')]
        manifest: bool,

        /// Filter by action type
        #[clap(short = 't')]
        action_type: Option<Vec<String>>,

        /// Publisher to show contents for
        #[clap(short = 'p')]
        publisher: Option<Vec<String>>,

        /// SSL key file
        #[clap(long)]
        key: Option<PathBuf>,

        /// SSL certificate file
        #[clap(long)]
        cert: Option<PathBuf>,

        /// Package FMRI patterns to match
        pkg_fmri_pattern: Option<Vec<String>>,
    },

    /// Rebuild repository metadata
    Rebuild {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Publisher to rebuild metadata for
        #[clap(short = 'p')]
        publisher: Option<Vec<String>>,

        /// SSL key file
        #[clap(long)]
        key: Option<PathBuf>,

        /// SSL certificate file
        #[clap(long)]
        cert: Option<PathBuf>,

        /// Skip catalog rebuild
        #[clap(long)]
        no_catalog: bool,

        /// Skip index rebuild
        #[clap(long)]
        no_index: bool,
    },

    /// Refresh repository metadata
    Refresh {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Publisher to refresh metadata for
        #[clap(short = 'p')]
        publisher: Option<Vec<String>>,

        /// SSL key file
        #[clap(long)]
        key: Option<PathBuf>,

        /// SSL certificate file
        #[clap(long)]
        cert: Option<PathBuf>,

        /// Skip catalog refresh
        #[clap(long)]
        no_catalog: bool,

        /// Skip index refresh
        #[clap(long)]
        no_index: bool,
    },

    /// Set repository properties
    Set {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Publisher to set properties for
        #[clap(short = 'p')]
        publisher: Option<String>,

        /// Properties to set (section/property=value)
        property_value: Vec<String>,
    },

    /// Search for packages in a repository
    Search {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,

        /// Output format
        #[clap(short = 'F')]
        format: Option<String>,

        /// Omit headers
        #[clap(short = 'H')]
        omit_headers: bool,

        /// Publisher to search packages for
        #[clap(short = 'p')]
        publisher: Option<Vec<String>>,

        /// SSL key file
        #[clap(long)]
        key: Option<PathBuf>,

        /// SSL certificate file
        #[clap(long)]
        cert: Option<PathBuf>,

        /// Maximum number of results to return
        #[clap(short = 'n', long = "limit")]
        limit: Option<usize>,

        /// Search query
        query: String,
    },

    /// Import a pkg5 repository
    ImportPkg5 {
        /// Path to the pkg5 repository (directory or p5p archive)
        #[clap(short = 's', long)]
        source: PathBuf,

        /// Path to the destination repository
        #[clap(short = 'd', long)]
        destination: PathBuf,

        /// Publisher to import (defaults to the first publisher found)
        #[clap(short = 'p', long)]
        publisher: Option<String>,
    },
    
    /// Mark a package as obsoleted
    ObsoletePackage {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Publisher of the package
        #[clap(short = 'p')]
        publisher: String,
        
        /// FMRI of the package to mark as obsoleted
        #[clap(short = 'f')]
        fmri: String,
        
        /// Optional deprecation message explaining why the package is obsoleted
        #[clap(short = 'm', long = "message")]
        message: Option<String>,
        
        /// Optional list of packages that replace this obsoleted package
        #[clap(short = 'r', long = "replaced-by")]
        replaced_by: Option<Vec<String>>,
    },
    
    /// List obsoleted packages in a repository
    ListObsoleted {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Output format
        #[clap(short = 'F')]
        format: Option<String>,
        
        /// Omit headers
        #[clap(short = 'H')]
        omit_headers: bool,
        
        /// Publisher to list obsoleted packages for
        #[clap(short = 'p')]
        publisher: String,
        
        /// Page number (1-based, defaults to 1)
        #[clap(long = "page")]
        page: Option<usize>,
        
        /// Number of packages per page (defaults to 100, 0 for all)
        #[clap(long = "page-size")]
        page_size: Option<usize>,
    },
    
    /// Show details of an obsoleted package
    ShowObsoleted {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Output format
        #[clap(short = 'F')]
        format: Option<String>,
        
        /// Publisher of the package
        #[clap(short = 'p')]
        publisher: String,
        
        /// FMRI of the obsoleted package to show
        #[clap(short = 'f')]
        fmri: String,
    },
    
    /// Search for obsoleted packages
    SearchObsoleted {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Output format
        #[clap(short = 'F')]
        format: Option<String>,
        
        /// Omit headers
        #[clap(short = 'H')]
        omit_headers: bool,
        
        /// Publisher to search obsoleted packages for
        #[clap(short = 'p')]
        publisher: String,
        
        /// Search pattern (supports glob patterns)
        #[clap(short = 'q')]
        pattern: String,
        
        /// Maximum number of results to return
        #[clap(short = 'n', long = "limit")]
        limit: Option<usize>,
    },
    
    /// Restore an obsoleted package to the main repository
    RestoreObsoleted {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Publisher of the package
        #[clap(short = 'p')]
        publisher: String,
        
        /// FMRI of the obsoleted package to restore
        #[clap(short = 'f')]
        fmri: String,
        
        /// Skip rebuilding the catalog after restoration
        #[clap(long = "no-rebuild")]
        no_rebuild: bool,
    },
    
    /// Export obsoleted packages to a file
    ExportObsoleted {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Publisher to export obsoleted packages for
        #[clap(short = 'p')]
        publisher: String,
        
        /// Output file path
        #[clap(short = 'o')]
        output_file: String,
        
        /// Optional search pattern to filter packages
        #[clap(short = 'q')]
        pattern: Option<String>,
    },
    
    /// Import obsoleted packages from a file
    ImportObsoleted {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Input file path
        #[clap(short = 'i')]
        input_file: String,
        
        /// Override publisher (use this instead of the one in the export file)
        #[clap(short = 'p')]
        publisher: Option<String>,
    },
    
    /// Clean up obsoleted packages older than a specified TTL (time-to-live)
    CleanupObsoleted {
        /// Path or URI of the repository
        #[clap(short = 's')]
        repo_uri_or_path: String,
        
        /// Publisher to clean up obsoleted packages for
        #[clap(short = 'p')]
        publisher: String,
        
        /// TTL in days
        #[clap(short = 't', long = "ttl-days", default_value = "90")]
        ttl_days: u32,
        
        /// Perform a dry run (don't actually remove packages)
        #[clap(short = 'n', long = "dry-run")]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    // Initialize the tracing subscriber with the default log level as debug and no decorations
    // Parse the environment filter first, handling any errors with our custom error type
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .from_env()
        .map_err(|e| {
            Pkg6RepoError::LoggingEnvError(format!("Failed to parse environment filter: {}", e))
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
        Commands::Create {
            repo_version,
            uri_or_path,
        } => {
            info!(
                "Creating repository version {} at {}",
                repo_version, uri_or_path
            );

            // Convert repo_version to RepositoryVersion
            let repo_version_enum = RepositoryVersion::try_from(*repo_version)?;

            // Create the repository
            let repo = FileBackend::create(uri_or_path, repo_version_enum)?;

            info!("Repository created successfully at {}", repo.path.display());
            Ok(())
        }
        Commands::AddPublisher {
            repo_uri_or_path,
            publisher,
        } => {
            info!(
                "Adding publishers {:?} to repository {}",
                publisher, repo_uri_or_path
            );

            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;

            // Add each publisher
            for p in publisher {
                info!("Adding publisher: {}", p);
                repo.add_publisher(p)?;
            }

            info!("Publishers added successfully");
            Ok(())
        }
        Commands::RemovePublisher {
            repo_uri_or_path,
            dry_run,
            publisher,
        } => {
            info!(
                "Removing publishers {:?} from repository {}",
                publisher, repo_uri_or_path
            );
            debug!("Dry run: {}", dry_run);

            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;

            // Remove each publisher
            for p in publisher {
                info!("Removing publisher: {}", p);
                repo.remove_publisher(p, *dry_run)?;
            }

            if *dry_run {
                info!("Dry run completed. No changes were made.");
            } else {
                info!("Publishers removed successfully");
            }

            Ok(())
        }
        Commands::Get {
            repo_uri_or_path,
            format,
            omit_headers,
            publisher,
            section_property,
            ..
        } => {
            info!("Getting properties from repository {}", repo_uri_or_path);

            // Open the repository
            // In a real implementation with RestBackend, the key and cert parameters would be used for SSL authentication
            // For now, we're using FileBackend, which doesn't use these parameters
            let repo = FileBackend::open(repo_uri_or_path)?;

            // Process the publisher parameter
            let pub_names = if let Some(publishers) = publisher {
                if !publishers.is_empty() {
                    publishers.clone()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            // Filter properties if section_property is specified
            let section_filtered_properties = if let Some(section_props) = section_property {
                let mut filtered = std::collections::HashMap::new();

                for section_prop in section_props {
                    // Check if the section_property contains a slash (section/property)
                    if section_prop.contains('/') {
                        // Exact match for section/property
                        if let Some(value) = repo.config.properties.get(section_prop) {
                            filtered.insert(section_prop.clone(), value.clone());
                        }
                    } else {
                        // Match section only
                        for (key, value) in &repo.config.properties {
                            if key.starts_with(&format!("{}/", section_prop)) {
                                filtered.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }

                filtered
            } else {
                // No filtering, use all properties
                repo.config.properties.clone()
            };

            // Filter properties by publisher if specified
            let filtered_properties = if !pub_names.is_empty() {
                let mut filtered = std::collections::HashMap::new();

                for (key, value) in &section_filtered_properties {
                    let parts: Vec<&str> = key.split('/').collect();
                    if parts.len() == 2 && pub_names.contains(&parts[0].to_string()) {
                        filtered.insert(key.clone(), value.clone());
                    }
                }

                filtered
            } else {
                section_filtered_properties
            };

            // Determine the output format
            let output_format = format.as_deref().unwrap_or("table");

            match output_format {
                "table" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("{:<10} {:<10} {:<20}", "SECTION", "PROPERTY", "VALUE");
                    }

                    // Print repository properties
                    for (key, value) in &filtered_properties {
                        let parts: Vec<&str> = key.split('/').collect();
                        if parts.len() == 2 {
                            println!("{:<10} {:<10} {:<20}", parts[0], parts[1], value);
                        } else {
                            println!("{:<10} {:<10} {:<20}", "", key, value);
                        }
                    }
                }
                "json" => {
                    // Create a JSON representation of the properties using serde_json
                    let properties_output = PropertiesOutput {
                        properties: repo.config.properties.clone(),
                    };

                    // Serialize to pretty-printed JSON
                    let json_output = serde_json::to_string_pretty(&properties_output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));

                    println!("{}", json_output);
                }
                "tsv" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("SECTION\tPROPERTY\tVALUE");
                    }

                    // Print repository properties as tab-separated values
                    for (key, value) in &repo.config.properties {
                        let parts: Vec<&str> = key.split('/').collect();
                        if parts.len() == 2 {
                            println!("{}\t{}\t{}", parts[0], parts[1], value);
                        } else {
                            println!("\t{}\t{}", key, value);
                        }
                    }
                }
                _ => {
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(
                        output_format.to_string(),
                    ));
                }
            }

            Ok(())
        }
        Commands::Info {
            repo_uri_or_path,
            format,
            omit_headers,
            publisher,
            ..
        } => {
            info!("Displaying info for repository {}", repo_uri_or_path);

            // Open the repository
            // In a real implementation with RestBackend, the key and cert parameters would be used for SSL authentication
            // For now, we're using FileBackend, which doesn't use these parameters
            let repo = FileBackend::open(repo_uri_or_path)?;

            // Process the publisher parameter
            let pub_names = if let Some(publishers) = publisher {
                if !publishers.is_empty() {
                    publishers.clone()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            // Get repository info
            let mut repo_info = repo.get_info()?;

            // Filter publishers if specified
            if !pub_names.is_empty() {
                repo_info.publishers.retain(|p| pub_names.contains(&p.name));
            }

            // Determine the output format
            let output_format = format.as_deref().unwrap_or("table");

            match output_format {
                "table" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!(
                            "{:<10} {:<8} {:<6} {:<30}",
                            "PUBLISHER", "PACKAGES", "STATUS", "UPDATED"
                        );
                    }

                    // Print repository info
                    for publisher_info in repo_info.publishers {
                        println!(
                            "{:<10} {:<8} {:<6} {:<30}",
                            publisher_info.name,
                            publisher_info.package_count,
                            publisher_info.status,
                            publisher_info.updated
                        );
                    }
                }
                "json" => {
                    // Create a JSON representation of the repository info using serde_json
                    let info_output = InfoOutput {
                        publishers: repo_info.publishers,
                    };

                    // Serialize to pretty-printed JSON
                    let json_output = serde_json::to_string_pretty(&info_output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));

                    println!("{}", json_output);
                }
                "tsv" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("PUBLISHER\tPACKAGES\tSTATUS\tUPDATED");
                    }

                    // Print repository info as tab-separated values
                    for publisher_info in repo_info.publishers {
                        println!(
                            "{}\t{}\t{}\t{}",
                            publisher_info.name,
                            publisher_info.package_count,
                            publisher_info.status,
                            publisher_info.updated
                        );
                    }
                }
                _ => {
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(
                        output_format.to_string(),
                    ));
                }
            }

            Ok(())
        }
        Commands::List {
            repo_uri_or_path,
            format,
            omit_headers,
            publisher,
            pkg_fmri_pattern,
            ..
        } => {
            info!("Listing packages in repository {}", repo_uri_or_path);

            // Open the repository
            // In a real implementation with RestBackend, the key and cert parameters would be used for SSL authentication
            // For now, we're using FileBackend, which doesn't use these parameters
            let repo = FileBackend::open(repo_uri_or_path)?;

            // Get the publisher if specified
            let pub_option = if let Some(publishers) = publisher {
                if !publishers.is_empty() {
                    Some(publishers[0].as_str())
                } else {
                    None
                }
            } else {
                None
            };

            // Get the pattern if specified
            let pattern_option = if let Some(patterns) = pkg_fmri_pattern {
                if !patterns.is_empty() {
                    Some(patterns[0].as_str())
                } else {
                    None
                }
            } else {
                None
            };

            // List packages
            let packages = repo.list_packages(pub_option, pattern_option)?;

            // Determine the output format
            let output_format = format.as_deref().unwrap_or("table");

            match output_format {
                "table" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("{:<30} {:<15} {:<10}", "NAME", "VERSION", "PUBLISHER");
                    }

                    // Print packages
                    for pkg_info in packages {
                        // Format version and publisher, handling optional fields
                        let version_str = pkg_info.fmri.version();

                        let publisher_str = match &pkg_info.fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };

                        println!(
                            "{:<30} {:<15} {:<10}",
                            pkg_info.fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                }
                "json" => {
                    // Create a JSON representation of the packages using serde_json
                    let packages_output = PackagesOutput { packages };

                    // Serialize to pretty-printed JSON
                    let json_output = serde_json::to_string_pretty(&packages_output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));

                    println!("{}", json_output);
                }
                "tsv" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("NAME\tVERSION\tPUBLISHER");
                    }

                    // Print packages as tab-separated values
                    for pkg_info in packages {
                        // Format version and publisher, handling optional fields
                        let version_str = pkg_info.fmri.version();

                        let publisher_str = match &pkg_info.fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };

                        println!(
                            "{}\t{}\t{}",
                            pkg_info.fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                }
                _ => {
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(
                        output_format.to_string(),
                    ));
                }
            }

            Ok(())
        }
        Commands::Contents {
            repo_uri_or_path,
            manifest,
            action_type,
            publisher,
            pkg_fmri_pattern,
            ..
        } => {
            info!("Showing contents in repository {}", repo_uri_or_path);

            // Open the repository
            // In a real implementation with RestBackend, the key and cert parameters would be used for SSL authentication
            // For now, we're using FileBackend, which doesn't use these parameters
            let repo = FileBackend::open(repo_uri_or_path)?;

            // Get the publisher if specified
            let pub_option = if let Some(publishers) = publisher {
                if !publishers.is_empty() {
                    Some(publishers[0].as_str())
                } else {
                    None
                }
            } else {
                None
            };

            // Get the pattern if specified
            let pattern_option = if let Some(patterns) = pkg_fmri_pattern {
                if !patterns.is_empty() {
                    Some(patterns[0].as_str())
                } else {
                    None
                }
            } else {
                None
            };

            // Show contents
            let contents =
                repo.show_contents(pub_option, pattern_option, action_type.as_deref())?;

            // Print contents
            for pkg_contents in contents {
                // Process files
                if let Some(files) = &pkg_contents.files {
                    for path in files {
                        if *manifest {
                            // If a manifest option is specified, print in manifest format
                            println!("file path={} type={}", path, pkg_contents.package_id);
                        } else {
                            // Otherwise, print in table format
                            println!(
                                "{:<40} {:<30} {:<10}",
                                pkg_contents.package_id, path, "file"
                            );
                        }
                    }
                }

                // Process directories
                if let Some(directories) = &pkg_contents.directories {
                    for path in directories {
                        if *manifest {
                            // If a manifest option is specified, print in manifest format
                            println!("dir path={} type={}", path, pkg_contents.package_id);
                        } else {
                            // Otherwise, print in table format
                            println!("{:<40} {:<30} {:<10}", pkg_contents.package_id, path, "dir");
                        }
                    }
                }

                // Process links
                if let Some(links) = &pkg_contents.links {
                    for path in links {
                        if *manifest {
                            // If a manifest option is specified, print in manifest format
                            println!("link path={} type={}", path, pkg_contents.package_id);
                        } else {
                            // Otherwise, print in table format
                            println!(
                                "{:<40} {:<30} {:<10}",
                                pkg_contents.package_id, path, "link"
                            );
                        }
                    }
                }

                // Process dependencies
                if let Some(dependencies) = &pkg_contents.dependencies {
                    for path in dependencies {
                        if *manifest {
                            // If a manifest option is specified, print in manifest format
                            println!("depend path={} type={}", path, pkg_contents.package_id);
                        } else {
                            // Otherwise, print in table format
                            println!(
                                "{:<40} {:<30} {:<10}",
                                pkg_contents.package_id, path, "depend"
                            );
                        }
                    }
                }

                // Process licenses
                if let Some(licenses) = &pkg_contents.licenses {
                    for path in licenses {
                        if *manifest {
                            // If a manifest option is specified, print in manifest format
                            println!("license path={} type={}", path, pkg_contents.package_id);
                        } else {
                            // Otherwise, print in table format
                            println!(
                                "{:<40} {:<30} {:<10}",
                                pkg_contents.package_id, path, "license"
                            );
                        }
                    }
                }
            }

            Ok(())
        }
        Commands::Rebuild {
            repo_uri_or_path,
            publisher,
            no_catalog,
            no_index,
            ..
        } => {
            info!("Rebuilding repository {}", repo_uri_or_path);

            // Open the repository
            // In a real implementation with RestBackend, the key and cert parameters would be used for SSL authentication
            // For now, we're using FileBackend, which doesn't use these parameters
            let repo = FileBackend::open(repo_uri_or_path)?;

            // Get the publisher if specified
            let pub_option = if let Some(publishers) = publisher {
                if !publishers.is_empty() {
                    Some(publishers[0].as_str())
                } else {
                    None
                }
            } else {
                None
            };

            // Rebuild repository metadata
            repo.rebuild(pub_option, *no_catalog, *no_index)?;

            info!("Repository rebuilt successfully");
            Ok(())
        }
        Commands::Refresh {
            repo_uri_or_path,
            publisher,
            no_catalog,
            no_index,
            ..
        } => {
            info!("Refreshing repository {}", repo_uri_or_path);

            // Open the repository
            // In a real implementation with RestBackend, the key and cert parameters would be used for SSL authentication
            // For now, we're using FileBackend, which doesn't use these parameters
            let repo = FileBackend::open(repo_uri_or_path)?;

            // Get the publisher if specified
            let pub_option = if let Some(publishers) = publisher {
                if !publishers.is_empty() {
                    Some(publishers[0].as_str())
                } else {
                    None
                }
            } else {
                None
            };

            // Refresh repository metadata
            repo.refresh(pub_option, *no_catalog, *no_index)?;

            info!("Repository refreshed successfully");
            Ok(())
        }
        Commands::Set {
            repo_uri_or_path,
            publisher,
            property_value,
        } => {
            info!("Setting properties for repository {}", repo_uri_or_path);

            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;

            // Process each property=value pair
            for prop_val in property_value {
                // Split the property=value string
                let parts: Vec<&str> = prop_val.split('=').collect();
                if parts.len() != 2 {
                    return Err(Pkg6RepoError::InvalidPropertyValueFormat(
                        prop_val.to_string(),
                    ));
                }

                let property = parts[0];
                let value = parts[1];

                // If a publisher is specified, set the publisher property
                if let Some(pub_name) = publisher {
                    info!(
                        "Setting publisher property {}/{} = {}",
                        pub_name, property, value
                    );
                    repo.set_publisher_property(pub_name, property, value)?;
                } else {
                    // Otherwise, set the repository property
                    info!("Setting repository property {} = {}", property, value);
                    repo.set_property(property, value)?;
                }
            }

            info!("Properties set successfully");
            Ok(())
        }
        Commands::Search {
            repo_uri_or_path,
            format,
            omit_headers,
            publisher,
            limit,
            query,
            ..
        } => {
            info!("Searching for packages in repository {}", repo_uri_or_path);

            // Open the repository
            // In a real implementation with RestBackend, the key and cert parameters would be used for SSL authentication
            // For now, we're using FileBackend, which doesn't use these parameters
            let repo = FileBackend::open(repo_uri_or_path)?;

            // Get the publisher if specified
            let pub_option = if let Some(publishers) = publisher {
                if !publishers.is_empty() {
                    Some(publishers[0].as_str())
                } else {
                    None
                }
            } else {
                None
            };

            // Search for packages
            let packages = repo.search(&query, pub_option, *limit)?;

            // Determine the output format
            let output_format = format.as_deref().unwrap_or("table");

            match output_format {
                "table" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("{:<30} {:<15} {:<10}", "NAME", "VERSION", "PUBLISHER");
                    }

                    // Print packages
                    for pkg_info in packages {
                        // Format version and publisher, handling optional fields
                        let version_str = pkg_info.fmri.version();

                        let publisher_str = match &pkg_info.fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };

                        println!(
                            "{:<30} {:<15} {:<10}",
                            pkg_info.fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                }
                "json" => {
                    // Create a JSON representation of the search results using serde_json
                    let search_output = SearchOutput {
                        query: query.clone(),
                        results: packages,
                    };

                    // Serialize to pretty-printed JSON
                    let json_output = serde_json::to_string_pretty(&search_output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));

                    println!("{}", json_output);
                }
                "tsv" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("NAME\tVERSION\tPUBLISHER");
                    }

                    // Print packages as tab-separated values
                    for pkg_info in packages {
                        // Format version and publisher, handling optional fields
                        let version_str = pkg_info.fmri.version();

                        let publisher_str = match &pkg_info.fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };

                        println!(
                            "{}\t{}\t{}",
                            pkg_info.fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                }
                _ => {
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(
                        output_format.to_string(),
                    ));
                }
            }

            Ok(())
        }
        Commands::ImportPkg5 {
            source,
            destination,
            publisher,
        } => {
            info!(
                "Importing pkg5 repository from {} to {}",
                source.display(),
                destination.display()
            );

            // Create a new Pkg5Importer
            let mut importer = Pkg5Importer::new(source, destination)?;

            // Import the repository
            importer.import(publisher.as_deref())?;

            info!("Repository imported successfully");
            Ok(())
        },
        
        Commands::ObsoletePackage {
            repo_uri_or_path,
            publisher,
            fmri,
            message,
            replaced_by,
        } => {
            info!("Marking package as obsoleted: {}", fmri);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Parse the FMRI
            let parsed_fmri = libips::fmri::Fmri::parse(fmri)?;
            
            // Get the manifest for the package
            let pkg_dir = repo.path.join("pkg").join(publisher).join(parsed_fmri.stem());
            let encoded_version = url_encode(&parsed_fmri.version());
            let manifest_path = pkg_dir.join(&encoded_version);
            
            if !manifest_path.exists() {
                return Err(Pkg6RepoError::from(format!(
                    "Package not found: {}",
                    parsed_fmri
                )));
            }
            
            // Read the manifest content
            let manifest_content = std::fs::read_to_string(&manifest_path)?;
            
            // Create a new scope for the obsoleted_manager to ensure it's dropped before we call repo.rebuild()
            {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // Store the obsoleted package
                obsoleted_manager.store_obsoleted_package(
                    publisher,
                    &parsed_fmri,
                    &manifest_content,
                    replaced_by.clone(),
                    message.clone(),
                )?;
            } // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            // Remove the original package from the repository
            std::fs::remove_file(&manifest_path)?;
            
            // Rebuild the catalog to reflect the changes
            repo.rebuild(Some(publisher), false, false)?;
            
            info!("Package marked as obsoleted successfully: {}", parsed_fmri);
            Ok(())
        },
        
        Commands::ListObsoleted {
            repo_uri_or_path,
            format,
            omit_headers,
            publisher,
            page,
            page_size,
        } => {
            info!("Listing obsoleted packages for publisher: {}", publisher);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Get the obsoleted packages in a new scope to avoid borrowing issues
            let paginated_result = {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // List obsoleted packages with pagination
                obsoleted_manager.list_obsoleted_packages_paginated(publisher, page.clone(), page_size.clone())?
            }; // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            // Determine the output format
            let output_format = format.as_deref().unwrap_or("table");
            
            match output_format {
                "table" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("{:<30} {:<15} {:<10}", "NAME", "VERSION", "PUBLISHER");
                    }
                    
                    // Print packages
                    for fmri in &paginated_result.packages {
                        // Format version and publisher, handling optional fields
                        let version_str = fmri.version();
                        
                        let publisher_str = match &fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };
                        
                        println!(
                            "{:<30} {:<15} {:<10}",
                            fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                    
                    // Print pagination information
                    println!("\nPage {} of {} (Total: {} packages)", 
                        paginated_result.page, 
                        paginated_result.total_pages, 
                        paginated_result.total_count);
                },
                "json" => {
                    // Create a JSON representation of the obsoleted packages with pagination info
                    #[derive(Serialize)]
                    struct PaginatedOutput {
                        packages: Vec<String>,
                        page: usize,
                        page_size: usize,
                        total_pages: usize,
                        total_count: usize,
                    }
                    
                    let packages_str: Vec<String> = paginated_result.packages.iter().map(|f| f.to_string()).collect();
                    let paginated_output = PaginatedOutput {
                        packages: packages_str,
                        page: paginated_result.page,
                        page_size: paginated_result.page_size,
                        total_pages: paginated_result.total_pages,
                        total_count: paginated_result.total_count,
                    };
                    
                    // Serialize to pretty-printed JSON
                    let json_output = serde_json::to_string_pretty(&paginated_output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                    
                    println!("{}", json_output);
                },
                "tsv" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("NAME\tVERSION\tPUBLISHER");
                    }
                    
                    // Print packages as tab-separated values
                    for fmri in &paginated_result.packages {
                        // Format version and publisher, handling optional fields
                        let version_str = fmri.version();
                        
                        let publisher_str = match &fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };
                        
                        println!(
                            "{}\t{}\t{}",
                            fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                    
                    // Print pagination information
                    println!("\nPAGE\t{}\nTOTAL_PAGES\t{}\nTOTAL_COUNT\t{}", 
                        paginated_result.page, 
                        paginated_result.total_pages, 
                        paginated_result.total_count);
                },
                _ => {
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(
                        output_format.to_string(),
                    ));
                }
            }
            
            Ok(())
        },
        
        Commands::ShowObsoleted {
            repo_uri_or_path,
            format,
            publisher,
            fmri,
        } => {
            info!("Showing details of obsoleted package: {}", fmri);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Parse the FMRI
            let parsed_fmri = libips::fmri::Fmri::parse(fmri)?;
            
            // Get the obsoleted package metadata in a new scope to avoid borrowing issues
            let metadata = {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // Get the obsoleted package metadata
                match obsoleted_manager.get_obsoleted_package_metadata(publisher, &parsed_fmri)? {
                    Some(metadata) => metadata,
                    None => {
                        return Err(Pkg6RepoError::from(format!(
                            "Obsoleted package not found: {}",
                            parsed_fmri
                        )));
                    }
                }
            }; // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            // Determine the output format
            let output_format = format.as_deref().unwrap_or("table");
            
            match output_format {
                "table" => {
                    println!("FMRI: {}", metadata.fmri);
                    println!("Status: {}", metadata.status);
                    println!("Obsolescence Date: {}", metadata.obsolescence_date);
                    
                    if let Some(msg) = &metadata.deprecation_message {
                        println!("Deprecation Message: {}", msg);
                    }
                    
                    if let Some(replacements) = &metadata.obsoleted_by {
                        println!("Replaced By:");
                        for replacement in replacements {
                            println!("  {}", replacement);
                        }
                    }
                    
                    println!("Metadata Version: {}", metadata.metadata_version);
                    println!("Content Hash: {}", metadata.content_hash);
                },
                "json" => {
                    // Create a JSON representation of the obsoleted package details
                    let details_output = ObsoletedPackageDetailsOutput {
                        fmri: metadata.fmri,
                        status: metadata.status,
                        obsolescence_date: metadata.obsolescence_date,
                        deprecation_message: metadata.deprecation_message,
                        obsoleted_by: metadata.obsoleted_by,
                        metadata_version: metadata.metadata_version,
                        content_hash: metadata.content_hash,
                    };
                    
                    // Serialize to pretty-printed JSON
                    let json_output = serde_json::to_string_pretty(&details_output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                    
                    println!("{}", json_output);
                },
                "tsv" => {
                    println!("FMRI\t{}", metadata.fmri);
                    println!("Status\t{}", metadata.status);
                    println!("ObsolescenceDate\t{}", metadata.obsolescence_date);
                    
                    if let Some(msg) = &metadata.deprecation_message {
                        println!("DeprecationMessage\t{}", msg);
                    }
                    
                    if let Some(replacements) = &metadata.obsoleted_by {
                        for (i, replacement) in replacements.iter().enumerate() {
                            println!("ReplacedBy{}\t{}", i + 1, replacement);
                        }
                    }
                    
                    println!("MetadataVersion\t{}", metadata.metadata_version);
                    println!("ContentHash\t{}", metadata.content_hash);
                },
                _ => {
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(
                        output_format.to_string(),
                    ));
                }
            }
            
            Ok(())
        },
        
        Commands::SearchObsoleted {
            repo_uri_or_path,
            format,
            omit_headers,
            publisher,
            pattern,
            limit,
        } => {
            info!("Searching for obsoleted packages: {} (publisher: {})", pattern, publisher);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Get the obsoleted packages in a new scope to avoid borrowing issues
            let obsoleted_packages = {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // Search for obsoleted packages
                let mut packages = obsoleted_manager.search_obsoleted_packages(publisher, pattern)?;
                
                // Apply limit if specified
                if let Some(max_results) = limit {
                    packages.truncate(*max_results);
                }
                
                packages
            }; // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            // Determine the output format
            let output_format = format.as_deref().unwrap_or("table");
            
            match output_format {
                "table" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("{:<30} {:<15} {:<10}", "NAME", "VERSION", "PUBLISHER");
                    }
                    
                    // Print packages
                    for fmri in obsoleted_packages {
                        // Format version and publisher, handling optional fields
                        let version_str = fmri.version();
                        
                        let publisher_str = match &fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };
                        
                        println!(
                            "{:<30} {:<15} {:<10}",
                            fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                },
                "json" => {
                    // Create a JSON representation of the obsoleted packages
                    let packages_str: Vec<String> = obsoleted_packages.iter().map(|f| f.to_string()).collect();
                    let packages_output = ObsoletedPackagesOutput {
                        packages: packages_str,
                    };
                    
                    // Serialize to pretty-printed JSON
                    let json_output = serde_json::to_string_pretty(&packages_output)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                    
                    println!("{}", json_output);
                },
                "tsv" => {
                    // Print headers if not omitted
                    if !omit_headers {
                        println!("NAME\tVERSION\tPUBLISHER");
                    }
                    
                    // Print packages as tab-separated values
                    for fmri in obsoleted_packages {
                        // Format version and publisher, handling optional fields
                        let version_str = fmri.version();
                        
                        let publisher_str = match &fmri.publisher {
                            Some(publisher) => publisher.clone(),
                            None => String::new(),
                        };
                        
                        println!(
                            "{}\t{}\t{}",
                            fmri.stem(),
                            version_str,
                            publisher_str
                        );
                    }
                },
                _ => {
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(
                        output_format.to_string(),
                    ));
                }
            }
            
            Ok(())
        },
        
        Commands::RestoreObsoleted {
            repo_uri_or_path,
            publisher,
            fmri,
            no_rebuild,
        } => {
            info!("Restoring obsoleted package: {} (publisher: {})", fmri, publisher);
            
            // Parse the FMRI
            let parsed_fmri = libips::fmri::Fmri::parse(fmri)?;
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Get the manifest content and remove the obsoleted package
            let manifest_content = {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // Get the manifest content and remove the obsoleted package
                obsoleted_manager.get_and_remove_obsoleted_package(publisher, &parsed_fmri)?
            }; // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            // Parse the manifest
            let manifest = libips::actions::Manifest::parse_string(manifest_content)?;
            
            // Begin a transaction
            let mut transaction = repo.begin_transaction()?;
            
            // Set the publisher for the transaction
            transaction.set_publisher(publisher);
            
            // Update the manifest in the transaction
            transaction.update_manifest(manifest);
            
            // Commit the transaction
            transaction.commit()?;
            
            // Rebuild the catalog if not disabled
            if !no_rebuild {
                info!("Rebuilding catalog...");
                repo.rebuild(Some(publisher), false, false)?;
            }
            
            info!("Package restored successfully: {}", parsed_fmri);
            Ok(())
        },
        
        Commands::ExportObsoleted {
            repo_uri_or_path,
            publisher,
            output_file,
            pattern,
        } => {
            info!("Exporting obsoleted packages for publisher: {}", publisher);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Export the obsoleted packages
            let count = {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // Export the obsoleted packages
                let output_path = PathBuf::from(output_file);
                obsoleted_manager.export_obsoleted_packages(
                    publisher,
                    pattern.as_deref(),
                    &output_path,
                )?
            }; // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            info!("Exported {} obsoleted packages to {}", count, output_file);
            Ok(())
        },
        
        Commands::ImportObsoleted {
            repo_uri_or_path,
            input_file,
            publisher,
        } => {
            info!("Importing obsoleted packages from {}", input_file);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Import the obsoleted packages
            let count = {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // Import the obsoleted packages
                let input_path = PathBuf::from(input_file);
                obsoleted_manager.import_obsoleted_packages(
                    &input_path,
                    publisher.as_deref(),
                )?
            }; // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            info!("Imported {} obsoleted packages", count);
            Ok(())
        },
        
        Commands::CleanupObsoleted {
            repo_uri_or_path,
            publisher,
            ttl_days,
            dry_run,
        } => {
            if *dry_run {
                info!("Dry run: Cleaning up obsoleted packages older than {} days for publisher: {}", 
                      ttl_days, publisher);
            } else {
                info!("Cleaning up obsoleted packages older than {} days for publisher: {}", 
                      ttl_days, publisher);
            }
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Clean up the obsoleted packages
            let count = {
                // Get the obsoleted package manager
                let obsoleted_manager = repo.get_obsoleted_manager()?;
                
                // Clean up the obsoleted packages
                obsoleted_manager.cleanup_obsoleted_packages_older_than_ttl(
                    publisher,
                    *ttl_days,
                    *dry_run,
                )?
            }; // obsoleted_manager is dropped here, releasing the mutable borrow on repo
            
            if *dry_run {
                info!("Dry run: Would remove {} obsoleted packages", count);
            } else {
                info!("Successfully removed {} obsoleted packages", count);
            }
            
            Ok(())
        }
    }
}
