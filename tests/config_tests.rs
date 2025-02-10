use mediaflow_proxy_light::config::Config;
use std::{env, fs};

fn setup() {
    env::remove_var("APP__SERVER__HOST");
    env::remove_var("APP__SERVER__PORT");
    env::remove_var("APP__AUTH__API_PASSWORD");
    env::remove_var("APP__PROXY__BUFFER_SIZE");
    env::remove_var("APP__PROXY__TRANSPORT_ROUTES");
}

#[test]
fn test_config_from_env() {
    setup();

    // Set environment variables
    env::set_var("APP__SERVER__HOST", "127.0.0.1");
    env::set_var("APP__SERVER__PORT", "8888"); // Match the default port in Config
    env::set_var("APP__AUTH__API_PASSWORD", "test_password");
    env::set_var("APP__PROXY__BUFFER_SIZE", "16384");

    let config = Config::from_env().unwrap();

    // Assert configuration values
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 8888); // Updated to match default port
    assert_eq!(config.auth.api_password, "test_password");
    assert_eq!(config.proxy.buffer_size, 16384);
}

#[test]
fn test_transport_routes_config() {
    setup();

    // Modify the JSON string to be a single line with escaped quotes
    let routes_json = r#"{"all://*.streaming.com":{"proxy":true,"proxy_url":"socks5://test-proxy:1080","verify_ssl":true}}"#;

    env::set_var("APP__PROXY__TRANSPORT_ROUTES", routes_json);

    let config = Config::from_env().unwrap();

    // Add debug prints to help diagnose the issue
    println!(
        "Environment variable value: {}",
        env::var("APP__PROXY__TRANSPORT_ROUTES").unwrap_or_default()
    );
    println!("Parsed routes: {:?}", config.proxy.transport_routes);

    let routes = config.proxy.transport_routes;

    // Test route configuration
    assert!(
        routes.contains_key("all://*.streaming.com"),
        "Expected key 'all://*.streaming.com' not found in routes"
    );

    let route = routes
        .get("all://*.streaming.com")
        .expect("Route should exist");

    assert!(route.proxy, "Proxy should be enabled");
    assert_eq!(
        route.proxy_url.as_deref(),
        Some("socks5://test-proxy:1080"),
        "Proxy URL doesn't match"
    );
    assert!(route.verify_ssl, "SSL verification should be enabled");
}

#[test]
fn test_transport_routes_from_toml() {
    setup();

    let config_content = r#"
[server]
host = "127.0.0.1"
port = 8888

[auth]
api_password = "test_password"

[proxy]
buffer_size = 16384

[proxy.transport_routes."all://*.streaming.com"]
proxy = true
proxy_url = "socks5://test-proxy:1080"
verify_ssl = true
"#;

    // Create a temporary directory for our test
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join("config.toml");

    // Write the config file
    fs::write(&config_path, config_content).expect("Failed to write config");

    // Set the config path environment variable
    env::set_var("CONFIG_PATH", config_path.to_str().unwrap());

    let config = Config::from_env().unwrap();

    println!(
        "Parsed routes from TOML: {:?}",
        config.proxy.transport_routes
    );

    let routes = config.proxy.transport_routes;

    // Test route configuration
    assert!(
        routes.contains_key("all://*.streaming.com"),
        "Expected key 'all://*.streaming.com' not found in routes"
    );

    let route = routes
        .get("all://*.streaming.com")
        .expect("Route should exist");

    assert!(route.proxy, "Proxy should be enabled");
    assert_eq!(
        route.proxy_url.as_deref(),
        Some("socks5://test-proxy:1080"),
        "Proxy URL doesn't match"
    );
    assert!(route.verify_ssl, "SSL verification should be enabled");
}
