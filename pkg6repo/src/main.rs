use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use std::convert::TryFrom;

use libips::repository::{Repository, RepositoryVersion, FileBackend, PublisherInfo, RepositoryInfo};

#[cfg(test)]
mod tests;

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
        #[clap(long, default_value = "4")]
        version: u32,
        
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
        
        /// Wait for operation to complete
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
}

fn main() -> Result<()> {
    let cli = App::parse();

    match &cli.command {
        Commands::Create { version, uri_or_path } => {
            println!("Creating repository version {} at {}", version, uri_or_path);
            
            // Convert version to RepositoryVersion
            let repo_version = RepositoryVersion::try_from(*version)?;
            
            // Create the repository
            let repo = FileBackend::create(uri_or_path, repo_version)?;
            
            println!("Repository created successfully at {}", repo.path.display());
            Ok(())
        },
        Commands::AddPublisher { repo_uri_or_path, publisher } => {
            println!("Adding publishers {:?} to repository {}", publisher, repo_uri_or_path);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Add each publisher
            for p in publisher {
                println!("Adding publisher: {}", p);
                repo.add_publisher(p)?;
            }
            
            println!("Publishers added successfully");
            Ok(())
        },
        Commands::RemovePublisher { repo_uri_or_path, dry_run, synchronous, publisher } => {
            println!("Removing publishers {:?} from repository {}", publisher, repo_uri_or_path);
            println!("Dry run: {}, Synchronous: {}", dry_run, synchronous);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Remove each publisher
            for p in publisher {
                println!("Removing publisher: {}", p);
                repo.remove_publisher(p, *dry_run)?;
            }
            
            if *dry_run {
                println!("Dry run completed. No changes were made.");
            } else {
                println!("Publishers removed successfully");
            }
            
            Ok(())
        },
        Commands::Get { repo_uri_or_path, format, omit_headers, publisher, key, cert, section_property } => {
            println!("Getting properties from repository {}", repo_uri_or_path);
            
            // Open the repository
            let repo = FileBackend::open(repo_uri_or_path)?;
            
            // Print headers if not omitted
            if !omit_headers {
                println!("{:<10} {:<10} {:<20}", "SECTION", "PROPERTY", "VALUE");
            }
            
            // Print repository properties
            for (key, value) in &repo.config.properties {
                let parts: Vec<&str> = key.split('/').collect();
                if parts.len() == 2 {
                    println!("{:<10} {:<10} {:<20}", parts[0], parts[1], value);
                } else {
                    println!("{:<10} {:<10} {:<20}", "", key, value);
                }
            }
            
            Ok(())
        },
        Commands::Info { repo_uri_or_path, format, omit_headers, publisher, key, cert } => {
            println!("Displaying info for repository {}", repo_uri_or_path);
            
            // Open the repository
            let repo = FileBackend::open(repo_uri_or_path)?;
            
            // Get repository info
            let repo_info = repo.get_info()?;
            
            // Print headers if not omitted
            if !omit_headers {
                println!("{:<10} {:<8} {:<6} {:<30}", "PUBLISHER", "PACKAGES", "STATUS", "UPDATED");
            }
            
            // Print repository info
            for publisher_info in repo_info.publishers {
                println!("{:<10} {:<8} {:<6} {:<30}", 
                    publisher_info.name, 
                    publisher_info.package_count, 
                    publisher_info.status, 
                    publisher_info.updated
                );
            }
            
            Ok(())
        },
        Commands::List { repo_uri_or_path, format, omit_headers, publisher, key, cert, pkg_fmri_pattern } => {
            println!("Listing packages in repository {}", repo_uri_or_path);
            
            // Open the repository
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
            
            // Print headers if not omitted
            if !omit_headers {
                println!("{:<30} {:<15} {:<10}", "NAME", "VERSION", "PUBLISHER");
            }
            
            // Print packages
            for (name, version, publisher) in packages {
                println!("{:<30} {:<15} {:<10}", name, version, publisher);
            }
            
            Ok(())
        },
        Commands::Contents { repo_uri_or_path, manifest, action_type, key, cert, pkg_fmri_pattern } => {
            println!("Showing contents in repository {}", repo_uri_or_path);
            
            // Open the repository
            let repo = FileBackend::open(repo_uri_or_path)?;
            
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
            let contents = repo.show_contents(None, pattern_option, action_type.as_deref())?;
            
            // Print contents
            for (package, path, action_type) in contents {
                if *manifest {
                    // If manifest option is specified, print in manifest format
                    println!("{} path={} type={}", action_type, path, package);
                } else {
                    // Otherwise, print in table format
                    println!("{:<40} {:<30} {:<10}", package, path, action_type);
                }
            }
            
            Ok(())
        },
        Commands::Rebuild { repo_uri_or_path, publisher, key, cert, no_catalog, no_index } => {
            println!("Rebuilding repository {}", repo_uri_or_path);
            
            // Open the repository
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
        },
        Commands::Refresh { repo_uri_or_path, publisher, key, cert, no_catalog, no_index } => {
            println!("Refreshing repository {}", repo_uri_or_path);
            
            // Open the repository
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
        },
        Commands::Set { repo_uri_or_path, publisher, property_value } => {
            println!("Setting properties for repository {}", repo_uri_or_path);
            
            // Open the repository
            let mut repo = FileBackend::open(repo_uri_or_path)?;
            
            // Process each property=value pair
            for prop_val in property_value {
                // Split the property=value string
                let parts: Vec<&str> = prop_val.split('=').collect();
                if parts.len() != 2 {
                    return Err(anyhow!("Invalid property=value format: {}", prop_val));
                }
                
                let property = parts[0];
                let value = parts[1];
                
                // If a publisher is specified, set the publisher property
                if let Some(pub_name) = publisher {
                    println!("Setting publisher property {}/{} = {}", pub_name, property, value);
                    repo.set_publisher_property(pub_name, property, value)?;
                } else {
                    // Otherwise, set the repository property
                    println!("Setting repository property {} = {}", property, value);
                    repo.set_property(property, value)?;
                }
            }
            
            println!("Properties set successfully");
            Ok(())
        },
    }
}