use crate::errors::DepotError;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, knuffel::Decode, Clone)]
pub struct Config {
    #[knuffel(child)]
    pub server: ServerConfig,
    #[knuffel(child)]
    pub repository: RepositoryConfig,
    #[knuffel(child)]
    pub telemetry: Option<TelemetryConfig>,
    #[knuffel(child)]
    pub publishers: Option<PublishersConfig>,
    #[knuffel(child)]
    pub admin: Option<AdminConfig>,
    #[knuffel(child)]
    pub oauth2: Option<Oauth2Config>,
}

#[derive(Debug, knuffel::Decode, Clone)]
pub struct ServerConfig {
    #[knuffel(child, unwrap(arguments))]
    pub bind: Vec<String>,
    #[knuffel(child, unwrap(argument))]
    pub workers: Option<usize>,
    #[knuffel(child, unwrap(argument))]
    pub max_connections: Option<usize>,
    #[knuffel(child, unwrap(argument))]
    pub reuseport: Option<bool>,
    /// Default max-age for Cache-Control headers (seconds)
    #[knuffel(child, unwrap(argument))]
    pub cache_max_age: Option<u64>,
    #[knuffel(child, unwrap(argument))]
    pub tls_cert: Option<PathBuf>,
    #[knuffel(child, unwrap(argument))]
    pub tls_key: Option<PathBuf>,
}

#[derive(Debug, knuffel::Decode, Clone)]
pub struct RepositoryConfig {
    #[knuffel(child, unwrap(argument))]
    pub root: PathBuf,
    #[knuffel(child, unwrap(argument))]
    pub mode: Option<String>,
}

#[derive(Debug, knuffel::Decode, Clone)]
pub struct TelemetryConfig {
    #[knuffel(child, unwrap(argument))]
    pub otlp_endpoint: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub service_name: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub log_format: Option<String>,
}

#[derive(Debug, knuffel::Decode, Clone)]
pub struct PublishersConfig {
    #[knuffel(child, unwrap(arguments))]
    pub list: Vec<String>,
}

#[derive(Debug, knuffel::Decode, Clone)]
pub struct AdminConfig {
    #[knuffel(child, unwrap(argument))]
    pub unix_socket: Option<PathBuf>,
    /// If true, require Authorization on /admin/health as well
    #[knuffel(child, unwrap(argument))]
    pub require_auth_for_health: Option<bool>,
}

#[derive(Debug, knuffel::Decode, Clone)]
pub struct Oauth2Config {
    #[knuffel(child, unwrap(argument))]
    pub issuer: Option<String>,
    #[knuffel(child, unwrap(argument))]
    pub jwks_uri: Option<String>,
    #[knuffel(child, unwrap(arguments))]
    pub required_scopes: Option<Vec<String>>,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> crate::errors::Result<Self> {
        let path = path.unwrap_or_else(|| PathBuf::from("pkg6depotd.kdl"));

        let content = fs::read_to_string(&path).map_err(|e| {
            DepotError::Config(format!("Failed to read config file {:?}: {}", path, e))
        })?;

        knuffel::parse(path.to_str().unwrap_or("pkg6depotd.kdl"), &content)
            .map_err(|e| DepotError::Config(format!("Failed to parse config: {:?}", e)))
    }
}
