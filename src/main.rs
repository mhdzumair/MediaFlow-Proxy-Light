use std::sync::Arc;
use actix_web::{middleware, web, App, HttpServer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod config;
mod error;
mod models;
mod proxy;

use auth::middleware::AuthMiddleware;
use config::Config;
use proxy::{handler, stream::StreamManager};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::from_env().expect("Failed to load configuration");

    // Initialize auth middleware
    let auth_middleware = AuthMiddleware::new(config.auth.api_password.clone());

    // Initialize stream manager
    let stream_manager = StreamManager::new(config.proxy.clone());

    // Start HTTP server
    let server_config = Arc::new(config.clone());
    
    HttpServer::new(move || {
        let config = Arc::clone(&server_config);
        
        App::new()
            // Enable logger and compression middleware
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            // Enable authentication middleware
            .wrap(auth_middleware.clone())
            // Register shared data
            .app_data(web::Data::new(stream_manager.clone()))
            .app_data(web::Data::new(config))
            // Configure routes
            .service(
                web::scope("/proxy")
                    .route("/stream", web::get().to(handler::proxy_stream))
                    .route("/generate_url", web::post().to(handler::generate_url))
            )
            // Configure default error handlers
            .default_service(web::route().to(|| async {
                actix_web::HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Not Found"
                }))
            }))
    })
    .workers(config.server.workers)
    .bind((config.server.host.as_str(), config.server.port))?
    .run()
    .await
}