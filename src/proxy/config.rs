use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

use crate::config::ProxyConfig;

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
