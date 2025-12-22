use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "pkg6depotd")]
#[command(about = "IPS Package Depot Server", long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub no_daemon: bool,

    #[arg(long, value_name = "FILE")]
    pub pid_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the server (default)
    Start,
    /// Stop the running server
    Stop,
    /// Check server status
    Status,
    /// Reload configuration
    Reload,
    /// Test configuration
    ConfigTest,
    /// Check health
    Health,
    /// Admin commands
    Admin {
        #[command(subcommand)]
        cmd: AdminCommands,
    },
}

#[derive(Subcommand)]
pub enum AdminCommands {
    AuthCheck,
}
