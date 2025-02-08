use crate::proxy::config::ProxyRouteConfig;
use config::{Map, Value};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::warn;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    pub connect_timeout: u64,
    pub buffer_size: usize,
    pub follow_redirects: bool,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub all_proxy: bool,
    #[serde(default)]
    pub transport_routes: HashMap<String, ProxyRouteConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub api_password: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub proxy: ProxyConfig,
    pub auth: AuthConfig,
}

impl Config {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8888)?
            .set_default("server.workers", 4)?
            .set_default("proxy.connect_timeout", 30)?
            .set_default("proxy.buffer_size", 8192)?
            .set_default("proxy.follow_redirects", true)?
            .set_default("proxy.all_proxy", false)?
            .set_default("proxy.transport_routes", HashMap::<String, Value>::new())?
            .set_default("auth.api_password", "changeme")?;

        // Add configuration from file
        if let Ok(config_path) = std::env::var("CONFIG_PATH") {
            let path = Path::new(&config_path);
            if path.exists() {
                builder = builder.add_source(config::File::with_name(&config_path));
            } else {
                warn!("Config file not found at {}", config_path);
            }
        }

        // Add configuration from environment
        builder = builder.add_source(
            config::Environment::with_prefix("APP")
                .separator("__")
                .try_parsing(true),
        );

        // Handle TRANSPORT_ROUTES environment variable
        if let Ok(routes_json) = std::env::var("APP__PROXY__TRANSPORT_ROUTES") {
            match serde_json::from_str::<HashMap<String, ProxyRouteConfig>>(&routes_json) {
                Ok(routes) => {
                    // Convert to config::Map and config::Value
                    let routes_map = routes
                        .into_iter()
                        .map(|(k, v)| {
                            let mut inner_map = Map::new();
                            inner_map.insert("proxy".into(), Value::from(v.proxy));
                            if let Some(url) = v.proxy_url {
                                inner_map.insert("proxy_url".into(), Value::from(url));
                            }
                            inner_map.insert("verify_ssl".into(), Value::from(v.verify_ssl));
                            (k, Value::from(inner_map))
                        })
                        .collect::<Map<String, Value>>();

                    builder =
                        builder.set_override("proxy.transport_routes", Value::from(routes_map))?;
                }
                Err(e) => {
                    return Err(config::ConfigError::Message(format!(
                        "Failed to parse TRANSPORT_ROUTES: {}",
                        e
                    )));
                }
            }
        }

        // Build and deserialize the configuration
        let config = builder.build()?;
        config.try_deserialize()
    }
}
