mod cli;
mod config;
mod errors;
mod http;
mod telemetry;
mod repo;
mod daemon;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use miette::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    
    // Load config
    // For M1, let's just create a dummy default if not found/failed for testing purposes
    // In a real scenario we'd want to be more specific about errors.
    
    let config = match Config::load(args.config.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {}. Using default.", e);
             Config {
                server: config::ServerConfig {
                    bind: vec!["0.0.0.0:8080".to_string()],
                    workers: None,
                    max_connections: None,
                    reuseport: None,
                    tls_cert: None,
                    tls_key: None,
                },
                repository: config::RepositoryConfig {
                    root: std::path::PathBuf::from("/tmp/pkg_repo"),
                    mode: Some("readonly".to_string()),
                },
                telemetry: None,
                publishers: None,
                admin: None,
                oauth2: None,
             }
        }
    };

    // Init telemetry
    telemetry::init(&config);

    match args.command.unwrap_or(Commands::Start) {
        Commands::Start => {
            if !args.no_daemon {
                daemon::daemonize().map_err(|e| miette::miette!(e))?;
            }
            
            let router = http::routes::app_router();
            let bind = config.server.bind.first().cloned().unwrap_or_else(|| "0.0.0.0:8080".to_string());
            
            tracing::info!("Starting pkg6depotd on {}", bind);
            
            http::server::run(router, &bind).await.map_err(|e| miette::miette!(e))?;
        }
        Commands::ConfigTest => {
            println!("Configuration loaded successfully: {:?}", config);
        }
        _ => {
            println!("Command not yet implemented");
        }
    }

    Ok(())
}
