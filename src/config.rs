use config::{Map, Value};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::warn;
use url::Url;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ProxyRouteConfig {
    #[serde(default)]
    pub proxy: bool,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default = "default_verify_ssl")]
    pub verify_ssl: bool,
}

fn default_verify_ssl() -> bool {
    true
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

#[derive(Debug, Clone)]
pub struct ProxyRoute {
    pub pattern: Regex,
    pub config: ProxyRouteConfig,
}

#[derive(Debug, Clone)]
pub struct ProxyRouter {
    default_proxy: Option<String>,
    all_proxy: bool,
    routes: Vec<ProxyRoute>,
}

impl ProxyRouter {
    pub fn new(
        default_proxy: Option<String>,
        all_proxy: bool,
        routes_config: HashMap<String, ProxyRouteConfig>,
    ) -> Self {
        let mut routes = Vec::new();

        for (pattern, config) in routes_config {
            let pattern = pattern
                .replace(".", "\\.")
                .replace("*", "[^/]*")
                .replace("all://", "(http|https)://");

            match Regex::new(&format!("^{}", pattern)) {
                Ok(regex) => {
                    routes.push(ProxyRoute {
                        pattern: regex,
                        config,
                    });
                }
                Err(e) => {
                    tracing::error!("Invalid route pattern '{}': {}", pattern, e);
                }
            }
        }

        // Sort routes by specificity
        routes.sort_by(|a, b| {
            let a_wildcards = a.pattern.as_str().matches("[^/]*").count();
            let b_wildcards = b.pattern.as_str().matches("[^/]*").count();
            b_wildcards.cmp(&a_wildcards)
        });

        Self {
            default_proxy,
            all_proxy,
            routes,
        }
    }

    pub fn from_config(config: &ProxyConfig) -> Self {
        Self::new(
            config.proxy_url.clone(),
            config.all_proxy,
            config.transport_routes.clone(),
        )
    }

    pub fn get_proxy_config(&self, url: &str) -> Option<ProxyRouteConfig> {
        match Url::parse(url) {
            Ok(parsed_url) => {
                let url_str = parsed_url.as_str();

                // Check specific routes first
                for route in &self.routes {
                    if route.pattern.is_match(url_str) {
                        tracing::debug!("Matched route pattern: {}", route.pattern.as_str());
                        return Some(route.config.clone());
                    }
                }

                // If no specific route found and all_proxy is true, use default proxy
                if self.all_proxy {
                    return Some(ProxyRouteConfig {
                        proxy: true,
                        proxy_url: self.default_proxy.clone(),
                        verify_ssl: true,
                    });
                }
            }
            Err(e) => {
                tracing::error!("Failed to parse URL '{}': {}", url, e);
            }
        }

        None
    }

    pub fn default_proxy(&self) -> &Option<String> {
        &self.default_proxy
    }
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
