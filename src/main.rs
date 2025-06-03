use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{dev::Service, middleware, web, App, HttpServer};
use futures::FutureExt;
use std::sync::Arc;
use tracing_subscriber::{fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};

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
    // Initialize tracing with more detailed format
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,actix_web=debug".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE),
        )
        .try_init()
        .expect("Failed to initialize logging");

    // Load configuration
    let config = Config::from_env().expect("Failed to load configuration");

    // Initialize auth middleware
    let auth_middleware = AuthMiddleware::new(config.auth.api_password.clone());

    // Initialize stream manager
    let stream_manager = StreamManager::new(config.proxy.clone());

    // Start HTTP server
    let server_config = Arc::new(config.clone());

    tracing::info!(
        "Starting server on {}:{}",
        server_config.server.host,
        server_config.server.port
    );

    HttpServer::new(move || {
        let config = Arc::clone(&server_config);

        // Configure CORS to allow all domains
        let cors = Cors::permissive();

        App::new()
            // Enable CORS middleware
            .wrap(cors)
            // FastAPI-style access logs: IP:PORT - "METHOD PATH HTTP/VERSION" STATUS_CODE
            .wrap(Logger::new("%a - \"%r\" %s"))
            .wrap(middleware::Compress::default())
            .wrap(auth_middleware.clone())
            // Register shared data
            .app_data(web::Data::new(stream_manager.clone()))
            .app_data(web::Data::new(config))
            // Configure routes
            .service(
                web::scope("/proxy")
                    .route("/stream", web::get().to(handler::proxy_stream_get))
                    .route("/stream", web::head().to(handler::proxy_stream_head))
                    .route("/generate_url", web::post().to(handler::generate_url))
                    .route("/ip", web::get().to(handler::get_public_ip)),
            )
            .service(web::scope("/health").route("", web::get().to(|| async { "OK" })))
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
