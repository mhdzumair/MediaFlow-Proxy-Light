use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyRequest {
    pub destination: String,
    #[serde(default)]
    pub query_params: HashMap<String, String>,
    #[serde(default)]
    pub request_headers: HashMap<String, String>,
    #[serde(default)]
    pub response_headers: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateUrlRequest {
    pub mediaflow_proxy_url: String,
    pub endpoint: Option<String>,
    pub destination_url: String,
    #[serde(default)]
    pub query_params: HashMap<String, String>,
    #[serde(default)]
    pub request_headers: HashMap<String, String>,
    #[serde(default)]
    pub response_headers: HashMap<String, String>,
    pub expiration: Option<u64>,
    pub ip: Option<String>,
    pub api_password: Option<String>,
}

pub const SUPPORTED_RESPONSE_HEADERS: &[&str] = &[
    "accept-ranges",
    "content-type",
    "content-length",
    "content-range",
    "connection",
    "transfer-encoding",
    "last-modified",
    "etag",
    "cache-control",
    "expires",
];

pub const SUPPORTED_REQUEST_HEADERS: &[&str] = &["range", "if-range"];
