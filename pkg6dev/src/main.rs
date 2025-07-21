use clap::{Parser, Subcommand};
use libips::actions::{ActionError, File, Manifest};
use libips::repository::{Repository, FileBackend};

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::fs::{read_dir, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use userland::repology::find_newest_version;
use userland::{Component, Makefile};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct App {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    DiffComponent {
        component: String,
        #[clap(short)]
        replacements: Option<Vec<String>>,

        /// Place the file actions missing in the manifests but present in sample-manifest into this file
        #[clap(short = 'm')]
        output_manifest: Option<PathBuf>,
    },
    ShowComponent {
        component: String,
    },
    /// Publish a package to a repository
    Publish {
        /// Path to the manifest file
        #[clap(short = 'm', long)]
        manifest_path: PathBuf,

        /// Path to the prototype directory containing the files to publish
        #[clap(short = 'p', long)]
        prototype_dir: PathBuf,

        /// Path to the repository
        #[clap(short = 'r', long)]
        repo_path: PathBuf,

        /// Publisher name (defaults to "test" if not specified)
        #[clap(short = 'u', long)]
        publisher: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = App::parse();

    match &cli.command {
        Commands::ShowComponent { component } => show_component_info(component),
        Commands::DiffComponent {
            component,
            replacements,
            output_manifest,
        } => diff_component(component, replacements, output_manifest),
        Commands::Publish {
            manifest_path,
            prototype_dir,
            repo_path,
            publisher,
        } => publish_package(manifest_path, prototype_dir, repo_path, publisher),
    }
}

fn parse_tripplet_replacements(replacements: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in replacements
        .iter()
        .map(|str| {
            str.split_once(':')
                .map(|s| (s.0.to_owned(), s.1.to_owned()))
                .unwrap_or((String::new(), String::new()))
        })
        .collect::<Vec<(String, String)>>()
    {
        map.insert(pair.0, pair.1);
    }

    map
}

fn diff_component(
    component_path: impl AsRef<Path>,
    replacements: &Option<Vec<String>>,
    output_manifest: &Option<PathBuf>,
) -> Result<()> {
    let replacements = if let Some(replacements) = replacements {
        let map = parse_tripplet_replacements(replacements);
        Some(map)
    } else {
        None
    };

    let files = read_dir(&component_path)?;

    let manifest_files: Vec<String> = files
        .filter_map(std::result::Result::ok)
        .filter(|d| {
            if let Some(e) = d.path().extension() {
                e == "p5m"
            } else {
                false
            }
        })
        .map(|e| e.path().into_os_string().into_string().unwrap())
        .collect();

    let sample_manifest_file = &component_path
        .as_ref()
        .join("manifests/sample-manifest.p5m");

    let manifests_res: Result<Vec<Manifest>, ActionError> =
        manifest_files.iter().map(Manifest::parse_file).collect();

    let sample_manifest = Manifest::parse_file(sample_manifest_file)?;

    let manifests: Vec<Manifest> = manifests_res.unwrap();

    let missing_files =
        find_files_missing_in_manifests(&sample_manifest, manifests.clone(), &replacements)?;

    for f in missing_files.clone() {
        println!("file {} is missing in the manifests", f.path);
    }

    let removed_files =
        find_removed_files(&sample_manifest, manifests, &component_path, &replacements)?;

    for f in removed_files {
        println!(
            "file path={} has been removed from the sample-manifest",
            f.path
        );
    }

    if let Some(output_manifest) = output_manifest {
        let mut f = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(output_manifest)?;
        for action in missing_files {
            writeln!(&mut f, "file path={}", action.path)?;
        }
    }

    Ok(())
}

fn show_component_info<P: AsRef<Path>>(component_path: P) -> Result<()> {
    let makefile_path = component_path.as_ref().join("Makefile");

    let initial_makefile = Makefile::parse_single_file(makefile_path)?;
    let makefile = initial_makefile.parse_all()?;

    let mut name = String::new();

    let component = Component::new_from_makefile(&makefile)?;

    if let Some(var) = makefile.get("COMPONENT_NAME") {
        println!("Name: {}", var.replace('\n', "\n\t"));
        if let Some(component_name) = makefile.get_first_value_of_variable_by_name("COMPONENT_NAME")
        {
            name = component_name;
        }
    }

    if let Some(var) = makefile.get("COMPONENT_VERSION") {
        println!("Version: {}", var.replace('\n', "\n\t"));
        let latest_version = find_newest_version(&name);
        if latest_version.is_ok() {
            println!("Latest Version: {}", latest_version?);
        } else {
            println!(
                "Error: Could not get latest version info: {:?}",
                latest_version.unwrap_err()
            )
        }
    }

    if let Some(var) = makefile.get("BUILD_BITS") {
        println!("Build bits: {}", var.replace('\n', "\n\t"));
    }

    if let Some(var) = makefile.get("COMPONENT_BUILD_ACTION") {
        println!("Build action: {}", var.replace('\n', "\n\t"));
    }

    if let Some(var) = makefile.get("COMPONENT_PROJECT_URL") {
        println!("Project URl: {}", var.replace('\n', "\n\t"));
    }

    if let Some(var) = makefile.get("COMPONENT_ARCHIVE_URL") {
        println!("Source URl: {}", var.replace('\n', "\n\t"));
    }

    if let Some(var) = makefile.get("COMPONENT_ARCHIVE_HASH") {
        println!("Source Archive File Hash: {}", var.replace('\n', "\n\t"));
    }

    if let Some(var) = makefile.get("REQUIRED_PACKAGES") {
        println!("Dependencies:\n\t{}", var.replace('\n', "\n\t"));
    }

    if let Some(var) = makefile.get("COMPONENT_INSTALL_ACTION") {
        println!("Install Action:\n\t{}", var);
    }

    println!("Component: {:?}", component);

    Ok(())
}

// Show all files that have been removed in the sample-manifest
fn find_removed_files<P: AsRef<Path>>(
    sample_manifest: &Manifest,
    manifests: Vec<Manifest>,
    component_path: P,
    replacements: &Option<HashMap<String, String>>,
) -> Result<Vec<File>> {
    let f_map = make_file_map(sample_manifest.files.clone());
    let all_files: Vec<File> = manifests.iter().flat_map(|m| m.files.clone()).collect();

    let mut removed_files: Vec<File> = Vec::new();

    for f in all_files {
        match f.get_original_path() {
            Some(path) => {
                if !f_map.contains_key(replace_func(path.clone(), replacements).as_str())
                    && !component_path.as_ref().join(path).exists()
                {
                    removed_files.push(f)
                }
            }
            None => {
                if !f_map.contains_key(replace_func(f.path.clone(), replacements).as_str()) {
                    removed_files.push(f)
                }
            }
        }
    }

    Ok(removed_files)
}

// Show all files missing in the manifests that are in sample_manifest
fn find_files_missing_in_manifests(
    sample_manifest: &Manifest,
    manifests: Vec<Manifest>,
    replacements: &Option<HashMap<String, String>>,
) -> Result<Vec<File>> {
    let all_files: Vec<File> = manifests.iter().flat_map(|m| m.files.clone()).collect();
    let f_map = make_file_map(all_files);

    let mut missing_files: Vec<File> = Vec::new();

    for f in sample_manifest.files.clone() {
        match f.get_original_path() {
            Some(path) => {
                if !f_map.contains_key(replace_func(path, replacements).as_str()) {
                    missing_files.push(f)
                }
            }
            None => {
                if !f_map.contains_key(replace_func(f.path.clone(), replacements).as_str()) {
                    missing_files.push(f)
                }
            }
        }
    }

    Ok(missing_files)
}

fn replace_func(orig: String, replacements: &Option<HashMap<String, String>>) -> String {
    if let Some(replacements) = replacements {
        let mut replacement = orig.clone();
        for (i, (from, to)) in replacements.iter().enumerate() {
            let from: &str = &format!("$({})", from);
            if i == 0 {
                replacement = orig.replace(from, to);
            } else {
                replacement = replacement.replace(from, to);
            }
        }
        replacement
    } else {
        orig
    }
}

fn make_file_map(files: Vec<File>) -> HashMap<String, File> {
    files
        .iter()
        .map(|f| {
            let orig_path_opt = f.get_original_path();
            if orig_path_opt.is_none() {
                return (f.path.clone(), f.clone());
            }
            (orig_path_opt.unwrap(), f.clone())
        })
        .collect()
}

/// Publish a package to a repository
///
/// This function:
/// 1. Opens the repository at the specified path
/// 2. Parses the manifest file
/// 3. Uses the FileBackend's publish_files method to publish the files from the prototype directory
fn publish_package(
    manifest_path: &PathBuf,
    prototype_dir: &PathBuf,
    repo_path: &PathBuf,
    publisher: &Option<String>,
) -> Result<()> {
    // Check if the manifest file exists
    if !manifest_path.exists() {
        return Err(anyhow!("Manifest file does not exist: {}", manifest_path.display()));
    }

    // Check if the prototype directory exists
    if !prototype_dir.exists() {
        return Err(anyhow!("Prototype directory does not exist: {}", prototype_dir.display()));
    }

    // Parse the manifest file
    println!("Parsing manifest file: {}", manifest_path.display());
    let manifest = Manifest::parse_file(manifest_path)?;

    // Open the repository
    println!("Opening repository at: {}", repo_path.display());
    let repo = match FileBackend::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => {
            println!("Repository does not exist, creating a new one...");
            // Create a new repository with version 4
            FileBackend::create(repo_path, libips::repository::RepositoryVersion::V4)?
        }
    };

    // Determine which publisher to use
    let publisher_name = if let Some(pub_name) = publisher {
        // Use the explicitly specified publisher
        if !repo.config.publishers.contains(pub_name) {
            return Err(anyhow!("Publisher '{}' does not exist in the repository. Please add it first using pkg6repo add-publisher.", pub_name));
        }
        pub_name.clone()
    } else {
        // Use the default publisher
        match &repo.config.default_publisher {
            Some(default_pub) => default_pub.clone(),
            None => return Err(anyhow!("No default publisher set in the repository. Please specify a publisher using the --publisher option or set a default publisher."))
        }
    };

    // Begin a transaction
    println!("Beginning transaction for publisher: {}", publisher_name);
    let mut transaction = repo.begin_transaction()?;

    // Add files from the prototype directory to the transaction
    println!("Adding files from prototype directory: {}", prototype_dir.display());
    for file_action in manifest.files.iter() {
        // Construct the full path to the file in the prototype directory
        let file_path = prototype_dir.join(&file_action.path);
        
        // Check if the file exists
        if !file_path.exists() {
            println!("Warning: File does not exist in prototype directory: {}", file_path.display());
            continue;
        }
        
        // Add the file to the transaction
        println!("Adding file: {}", file_action.path);
        transaction.add_file(file_action.clone(), &file_path)?;
    }

    // Update the manifest in the transaction
    println!("Updating manifest in the transaction...");
    transaction.update_manifest(manifest);

    // Commit the transaction
    println!("Committing transaction...");
    transaction.commit()?;

    println!("Package published successfully!");
    Ok(())
}
