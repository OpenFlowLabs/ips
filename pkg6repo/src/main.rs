mod error;
use error::{Pkg6RepoError, Result};

use clap::{Parser, Subcommand};
use serde::Serialize;
use std::convert::TryFrom;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};
use tracing_subscriber::fmt;

use libips::repository::{FileBackend, ReadableRepository, RepositoryVersion, WritableRepository};

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

        /// Wait for the operation to complete
        #[clap(long)]
        synchronous: bool,

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
}

fn main() -> Result<()> {
    // Initialize the tracing subscriber with default log level as warning and no decorations
    fmt::Subscriber::builder()
        .with_max_level(tracing::Level::WARN)
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
            println!(
                "Creating repository version {} at {}",
                repo_version, uri_or_path
            );

            // Convert repo_version to RepositoryVersion
            let repo_version_enum = RepositoryVersion::try_from(*repo_version)?;

            // Create the repository
            let repo = FileBackend::create(uri_or_path, repo_version_enum)?;

            println!("Repository created successfully at {}", repo.path.display());
            Ok(())
        }
        Commands::AddPublisher {
            repo_uri_or_path,
            publisher,
        } => {
            println!(
                "Adding publishers {:?} to repository {}",
                publisher, repo_uri_or_path
            );

            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;

            // Add each publisher
            for p in publisher {
                println!("Adding publisher: {}", p);
                repo.add_publisher(p)?;
            }

            println!("Publishers added successfully");
            Ok(())
        }
        Commands::RemovePublisher {
            repo_uri_or_path,
            dry_run,
            synchronous,
            publisher,
        } => {
            println!(
                "Removing publishers {:?} from repository {}",
                publisher, repo_uri_or_path
            );
            println!("Dry run: {}, Synchronous: {}", dry_run, synchronous);

            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;

            // Remove each publisher
            for p in publisher {
                println!("Removing publisher: {}", p);
                repo.remove_publisher(p, *dry_run)?;
            }

            // The synchronous parameter is used to wait for the operation to complete before returning
            // For FileBackend, operations are already synchronous, so this parameter doesn't have any effect
            // For RestBackend, this would wait for the server to complete the operation before returning
            if *synchronous {
                println!("Operation completed synchronously");
            }

            if *dry_run {
                println!("Dry run completed. No changes were made.");
            } else {
                println!("Publishers removed successfully");
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
            println!("Getting properties from repository {}", repo_uri_or_path);

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
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(output_format.to_string()));
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
            println!("Displaying info for repository {}", repo_uri_or_path);

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
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(output_format.to_string()));
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
            println!("Listing packages in repository {}", repo_uri_or_path);

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
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(output_format.to_string()));
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
            println!("Showing contents in repository {}", repo_uri_or_path);

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
            println!("Rebuilding repository {}", repo_uri_or_path);

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

            println!("Repository rebuilt successfully");
            Ok(())
        }
        Commands::Refresh {
            repo_uri_or_path,
            publisher,
            no_catalog,
            no_index,
            ..
        } => {
            println!("Refreshing repository {}", repo_uri_or_path);

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

            println!("Repository refreshed successfully");
            Ok(())
        }
        Commands::Set {
            repo_uri_or_path,
            publisher,
            property_value,
        } => {
            println!("Setting properties for repository {}", repo_uri_or_path);

            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;

            // Process each property=value pair
            for prop_val in property_value {
                // Split the property=value string
                let parts: Vec<&str> = prop_val.split('=').collect();
                if parts.len() != 2 {
                    return Err(Pkg6RepoError::InvalidPropertyValueFormat(prop_val.to_string()));
                }

                let property = parts[0];
                let value = parts[1];

                // If a publisher is specified, set the publisher property
                if let Some(pub_name) = publisher {
                    println!(
                        "Setting publisher property {}/{} = {}",
                        pub_name, property, value
                    );
                    repo.set_publisher_property(pub_name, property, value)?;
                } else {
                    // Otherwise, set the repository property
                    println!("Setting repository property {} = {}", property, value);
                    repo.set_property(property, value)?;
                }
            }

            println!("Properties set successfully");
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
            println!("Searching for packages in repository {}", repo_uri_or_path);

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
                    return Err(Pkg6RepoError::UnsupportedOutputFormat(output_format.to_string()));
                }
            }

            Ok(())
        }
    }
}