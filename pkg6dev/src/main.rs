use clap::{Parser, Subcommand};
use libips::actions::{File, Manifest};

use anyhow::Result;
use std::collections::HashMap;
use std::fs::{read_dir};
use std::path::Path;
use userland::Makefile;
use userland::repology::{find_newest_version};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct App {
    #[clap(subcommand)]
    command: Commands
}

#[derive(Subcommand, Debug)]
enum Commands {
    DiffComponent{
        component: String,
    },
    ShowComponent{
        component: String,
    },
}

fn main() -> Result<()> {
    let cli = App::parse();

    match &cli.command {
        Commands::ShowComponent{ component } => {
            show_component_info(component)
        }
        Commands::DiffComponent{component} => {
            diff_component(component)
        }
    }
}

fn diff_component(component_path: impl AsRef<Path>) -> Result<()> {
    let files = read_dir(&component_path)?;

    let manifest_files: Vec<String> = files
        .filter_map(std::result::Result::ok)
        .filter(|d| if let Some(e) = d.path().extension() { e == "p5m" } else {false})
        .map(|e| e.path().into_os_string().into_string().unwrap()).collect();

    let sample_manifest_file = &component_path.as_ref().join("/manifests/sample-manifest.p5m");

    let manifests_res: Result<Vec<Manifest>> = manifest_files.iter().map(|f|{
        Manifest::parse_file(f.to_string())
    }).collect();

    let sample_manifest = Manifest::parse_file(sample_manifest_file)?;

    let manifests: Vec<Manifest> = manifests_res.unwrap();

    let missing_files = find_files_missing_in_manifests(&sample_manifest, manifests.clone())?;

    for f in missing_files {
        println!("file {} is missing in the manifests", f.path);
    }

    let removed_files = find_removed_files(&sample_manifest, manifests.clone(), &component_path)?;

    for f in removed_files {
        println!("file path={} has been removed from the sample-manifest", f.path);
    }

    Ok(())
}

fn show_component_info<P: AsRef<Path>>(component_path: P) -> Result<()> {
    let makefile_path = component_path.as_ref().join("Makefile");

    println!("{:?}", makefile_path);

    let makefile = Makefile::parse_file(&makefile_path)?;

    let mut name = String::new();

    if let Some(var) = makefile.variables.get("COMPONENT_NAME") {
        println!("Name: {}", var.join(" "));
        name = var.first().unwrap().to_string();
    }

    if let Some(var) = makefile.variables.get("COMPONENT_VERSION") {
        println!("Version: {}", var.join(" "));
        let latest_version = find_newest_version(&name);
        if latest_version.is_ok() {
            println!("Latest Version: {}", latest_version?);
        } else {
            eprintln!("{:?}", latest_version.unwrap_err())
        }
    }

    if let Some(var) = makefile.variables.get("BUILD_BITS") {
        println!("Build bits: {}", var.join(" "));
    }

    if let Some(var) = makefile.variables.get("COMPONENT_PROJECT_URL") {
        println!("Project URl: {}", var.join("\t"));
    }

    if let Some(var) = makefile.variables.get("COMPONENT_ARCHIVE_URL") {
        println!("Source URl: {}", var.join("\t"));
    }

    if let Some(var) = makefile.variables.get("COMPONENT_ARCHIVE_HASH") {
        println!("Source Archive File Hash: {}", var.join(" "));
    }

    if let Some(var) = makefile.variables.get("CONFIGURE_ENV") {
        println!("Configure Environment: {}", var.join("\n\t"));
    }

    if let Some(var) = makefile.variables.get("CONFIGURE_OPTIONS") {
        println!("./configure {}", var.join("\n\t"));
    }

    if let Some(var) = makefile.variables.get("REQUIRED_PACKAGES") {
        println!("Dependencies:\n\t{}", var.join("\n\t"));
    }

    Ok(())
}

// Show all files that have been removed in the sample-manifest
fn find_removed_files<P: AsRef<Path>>(sample_manifest: &Manifest, manifests: Vec<Manifest>, component_path: P) -> Result<Vec<File>> {
    let f_map = make_file_map(sample_manifest.files.clone());
    let all_files: Vec<File> = manifests.iter().map(|m| m.files.clone()).flatten().collect();

    let mut removed_files: Vec<File> = Vec::new();

    for f in all_files {
        match f.get_original_path() {
            Some(path) => {
                if !f_map.contains_key(path.as_str()) {
                    if !component_path.as_ref().join(path).exists() {
                        removed_files.push(f)
                    }
                }
            },
            None => {
                if !f_map.contains_key(f.path.as_str()) {
                    removed_files.push(f)
                }
            }
        }
    }

    Ok(removed_files)
}

// Show all files missing in the manifests that are in sample_manifest
fn find_files_missing_in_manifests(sample_manifest: &Manifest, manifests: Vec<Manifest>) -> Result<Vec<File>> {
    let all_files: Vec<File> = manifests.iter().map(|m| m.files.clone()).flatten().collect();
    let f_map = make_file_map(all_files);

    let mut missing_files: Vec<File> = Vec::new();

    for f in sample_manifest.files.clone() {
        match f.get_original_path() {
            Some(path) => {
                if !f_map.contains_key(path.as_str()) {
                    missing_files.push(f)
                }
            },
            None => {
                if !f_map.contains_key(f.path.as_str()) {
                    missing_files.push(f)
                }
            }
        }
    }

    Ok(missing_files)
}

fn make_file_map(files: Vec<File>) -> HashMap<String, File> {
    files.iter().map(|f| {
        let orig_path_opt = f.get_original_path();
        if orig_path_opt == None {
            return (f.path.clone(), f.clone());
        }
        (orig_path_opt.unwrap(), f.clone())
    }).collect()
}
