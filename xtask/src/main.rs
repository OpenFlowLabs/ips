use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

// Constants
const TEST_BASE_DIR: &str = "/tmp/pkg6_test";
const PROTOTYPE_DIR: &str = "/tmp/pkg6_test/prototype";
const MANIFEST_DIR: &str = "/tmp/pkg6_test/manifests";
const E2E_TEST_BIN_DIR: &str = "/tmp/pkg6_test/bin";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up the test environment for repository tests
    SetupTestEnv,

    /// Build the project
    Build {
        /// Build with release optimizations
        #[arg(short, long)]
        release: bool,

        /// Specific crate to build
        #[arg(short, long)]
        package: Option<String>,
    },

    /// Run tests
    Test {
        /// Run tests with release optimizations
        #[arg(short, long)]
        release: bool,

        /// Specific crate to test
        #[arg(short, long)]
        package: Option<String>,
    },

    /// Build binaries for end-to-end tests
    BuildE2E,

    /// Run end-to-end tests using pre-built binaries
    RunE2E {
        /// Specific test to run (runs all e2e tests if not specified)
        #[arg(short, long)]
        test: Option<String>,
    },

    /// Format code
    Fmt,

    /// Run clippy
    Clippy,

    /// Clean build artifacts
    Clean,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::SetupTestEnv => setup_test_env(),
        Commands::Build { release, package } => build(release, package),
        Commands::Test { release, package } => test(release, package),
        Commands::BuildE2E => build_e2e(),
        Commands::RunE2E { test } => run_e2e(test),
        Commands::Fmt => fmt(),
        Commands::Clippy => clippy(),
        Commands::Clean => clean(),
    }
}

/// Set up the test environment for repository tests
fn setup_test_env() -> Result<()> {
    println!("Setting up test environment...");

    // Clean up any existing test directories except the bin directory
    if Path::new(TEST_BASE_DIR).exists() {
        println!("Cleaning up existing test directory...");

        // Remove subdirectories individually, preserving the bin directory
        let entries = fs::read_dir(TEST_BASE_DIR).context("Failed to read test directory")?;
        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            // Skip the bin directory
            if path.is_dir() && path.file_name().unwrap_or_default() != "bin" {
                fs::remove_dir_all(&path)
                    .context(format!("Failed to remove directory: {:?}", path))?;
            } else if path.is_file() {
                fs::remove_file(&path).context(format!("Failed to remove file: {:?}", path))?;
            }
        }
    } else {
        // Create the base directory if it doesn't exist
        fs::create_dir_all(TEST_BASE_DIR).context("Failed to create test base directory")?;
    }

    // Create test directories
    println!("Creating test directories...");
    fs::create_dir_all(PROTOTYPE_DIR).context("Failed to create prototype directory")?;
    fs::create_dir_all(MANIFEST_DIR).context("Failed to create manifest directory")?;

    // Compile the applications
    println!("Compiling applications...");
    Command::new("cargo")
        .arg("build")
        .status()
        .context("Failed to compile applications")?;

    // Create a simple prototype directory structure with some files
    println!("Creating prototype directory structure...");

    // Create some directories
    fs::create_dir_all(format!("{}/usr/bin", PROTOTYPE_DIR))
        .context("Failed to create usr/bin directory")?;
    fs::create_dir_all(format!("{}/usr/share/doc/example", PROTOTYPE_DIR))
        .context("Failed to create usr/share/doc/example directory")?;
    fs::create_dir_all(format!("{}/etc/config", PROTOTYPE_DIR))
        .context("Failed to create etc/config directory")?;

    // Create some files
    let hello_script = "#!/bin/sh\necho 'Hello, World!'";
    let mut hello_file = File::create(format!("{}/usr/bin/hello", PROTOTYPE_DIR))
        .context("Failed to create hello script")?;
    hello_file
        .write_all(hello_script.as_bytes())
        .context("Failed to write hello script")?;

    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(format!("{}/usr/bin/hello", PROTOTYPE_DIR))
            .context("Failed to get hello script metadata")?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(format!("{}/usr/bin/hello", PROTOTYPE_DIR), perms)
            .context("Failed to set hello script permissions")?;
    }

    let readme_content = "This is an example document.";
    let mut readme_file = File::create(format!(
        "{}/usr/share/doc/example/README.txt",
        PROTOTYPE_DIR
    ))
    .context("Failed to create README.txt")?;
    readme_file
        .write_all(readme_content.as_bytes())
        .context("Failed to write README.txt")?;

    let config_content = "# Example configuration file\nvalue=42";
    let mut config_file = File::create(format!("{}/etc/config/example.conf", PROTOTYPE_DIR))
        .context("Failed to create example.conf")?;
    config_file
        .write_all(config_content.as_bytes())
        .context("Failed to write example.conf")?;

    // Create a simple manifest
    println!("Creating package manifest...");
    let example_manifest = r#"set name=pkg.fmri value=pkg://test/example@1.0.0
set name=pkg.summary value="Example package for testing"
set name=pkg.description value="This is an example package used for testing the repository implementation."
set name=info.classification value="org.opensolaris.category.2008:System/Core"
set name=variant.arch value=i386 value=sparc
file path=usr/bin/hello mode=0755 owner=root group=bin
file path=usr/share/doc/example/README.txt mode=0644 owner=root group=bin
file path=etc/config/example.conf mode=0644 owner=root group=bin preserve=true
dir path=usr/bin mode=0755 owner=root group=bin
dir path=usr/share/doc/example mode=0755 owner=root group=bin
dir path=etc/config mode=0755 owner=root group=sys
"#;

    let mut example_file = File::create(format!("{}/example.p5m", MANIFEST_DIR))
        .context("Failed to create example.p5m")?;
    example_file
        .write_all(example_manifest.as_bytes())
        .context("Failed to write example.p5m")?;

    // Create a second manifest for testing multiple packages
    let example2_manifest = r#"set name=pkg.fmri value=pkg://test/example2@1.0.0
set name=pkg.summary value="Second example package for testing"
set name=pkg.description value="This is a second example package used for testing the repository implementation."
set name=info.classification value="org.opensolaris.category.2008:System/Core"
set name=variant.arch value=i386 value=sparc
file path=usr/bin/hello mode=0755 owner=root group=bin
file path=usr/share/doc/example/README.txt mode=0644 owner=root group=bin
dir path=usr/bin mode=0755 owner=root group=bin
dir path=usr/share/doc/example mode=0755 owner=root group=bin
"#;

    let mut example2_file = File::create(format!("{}/example2.p5m", MANIFEST_DIR))
        .context("Failed to create example2.p5m")?;
    example2_file
        .write_all(example2_manifest.as_bytes())
        .context("Failed to write example2.p5m")?;

    println!("Test environment setup complete!");
    println!("Prototype directory: {}", PROTOTYPE_DIR);
    println!("Manifest directory: {}", MANIFEST_DIR);

    Ok(())
}

/// Build the project
fn build(release: &bool, package: &Option<String>) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");

    if *release {
        cmd.arg("--release");
    }

    if let Some(pkg) = package {
        cmd.args(["--package", pkg]);
    }

    cmd.status().context("Failed to build project")?;

    Ok(())
}

/// Run tests
fn test(release: &bool, package: &Option<String>) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("test");

    if *release {
        cmd.arg("--release");
    }

    if let Some(pkg) = package {
        cmd.args(["--package", pkg]);
    }

    cmd.status().context("Failed to run tests")?;

    Ok(())
}

/// Format code
fn fmt() -> Result<()> {
    Command::new("cargo")
        .arg("fmt")
        .status()
        .context("Failed to format code")?;

    Ok(())
}

/// Run clippy
fn clippy() -> Result<()> {
    Command::new("cargo")
        .args([
            "clippy",
            "--all-targets",
            "--all-features",
        ])
        .status()
        .context("Failed to run clippy")?;

    Ok(())
}

/// Clean build artifacts
fn clean() -> Result<()> {
    Command::new("cargo")
        .arg("clean")
        .status()
        .context("Failed to clean build artifacts")?;

    Ok(())
}

/// Build binaries for end-to-end tests
fn build_e2e() -> Result<()> {
    println!("Building binaries for end-to-end tests...");

    // Create the bin directory if it doesn't exist
    fs::create_dir_all(E2E_TEST_BIN_DIR).context("Failed to create bin directory")?;

    // Build pkg6repo in release mode
    println!("Building pkg6repo...");
    Command::new("cargo")
        .args(["build", "--release", "--package", "pkg6repo"])
        .status()
        .context("Failed to build pkg6repo")?;

    // Build pkg6 in release mode
    println!("Building pkg6...");
    Command::new("cargo")
        .args(["build", "--release", "--package", "pkg6"])
        .status()
        .context("Failed to build pkg6")?;

    // Copy the binaries to the bin directory
    let target_dir = PathBuf::from("target/release");

    println!("Copying binaries to test directory...");
    fs::copy(
        target_dir.join("pkg6repo"),
        PathBuf::from(E2E_TEST_BIN_DIR).join("pkg6repo"),
    )
    .context("Failed to copy pkg6repo binary")?;

    fs::copy(
        target_dir.join("pkg6"),
        PathBuf::from(E2E_TEST_BIN_DIR).join("pkg6"),
    )
    .context("Failed to copy pkg6 binary")?;

    println!("End-to-end test binaries built successfully!");
    println!("Binaries are located at: {}", E2E_TEST_BIN_DIR);

    Ok(())
}

/// Run end-to-end tests using pre-built binaries
fn run_e2e(test: &Option<String>) -> Result<()> {
    println!("Running end-to-end tests...");

    // Check if the binaries exist
    let pkg6repo_bin = PathBuf::from(E2E_TEST_BIN_DIR).join("pkg6repo");
    let pkg6_bin = PathBuf::from(E2E_TEST_BIN_DIR).join("pkg6");

    if !pkg6repo_bin.exists() || !pkg6_bin.exists() {
        println!("Pre-built binaries not found. Building them first...");
        build_e2e()?;
    }

    // Set up the test environment
    setup_test_env()?;

    // Run the tests
    let mut cmd = Command::new("cargo");
    cmd.arg("test");

    if let Some(test_name) = test {
        cmd.args(["--package", "pkg6repo", "--test", "e2e_tests", test_name]);
    } else {
        cmd.args(["--package", "pkg6repo", "--test", "e2e_tests"]);
    }

    // Set the environment variable for the test binaries
    cmd.env("PKG6_TEST_BIN_DIR", E2E_TEST_BIN_DIR);

    cmd.status().context("Failed to run end-to-end tests")?;

    println!("End-to-end tests completed!");

    Ok(())
}
