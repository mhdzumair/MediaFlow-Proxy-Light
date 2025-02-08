use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub proxy: ProxyConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    pub connect_timeout: u64,
    pub stream_timeout: u64,
    pub follow_redirects: bool,
    pub buffer_size: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub api_password: String,
}

impl Config {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8888)?
            .set_default("server.workers", num_cpus::get() as i64)?
            .set_default("proxy.connect_timeout", 30)?
            .set_default("proxy.stream_timeout", 60)?
            .set_default("proxy.follow_redirects", true)?
            .set_default("proxy.buffer_size", 8192)?
            .set_default("auth.token_expiration", 3600)?;

        // Add configuration from file
        if let Ok(config_path) = env::var("CONFIG_PATH") {
            builder = builder.add_source(config::File::with_name(&config_path));
        }

        // Add configuration from environment
        builder = builder.add_source(
            config::Environment::with_prefix("APP")
                .separator("_")
                .try_parsing(true),
        );

        builder.build()?.try_deserialize()
    }
}