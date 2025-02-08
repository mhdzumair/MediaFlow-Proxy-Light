use actix_web::{HttpResponse, ResponseError};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Proxy error: {0}")]
    Proxy(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Upstream service error: {0}")]
    Upstream(String),

    #[error("Serde JSON error: {0}")]
    SerdeJsonError(serde_json::Error),
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::Auth(msg) => HttpResponse::Unauthorized().json(json!({ "error": msg })),
            AppError::Proxy(msg) => HttpResponse::BadGateway().json(json!({ "error": msg })),
            AppError::Internal(msg) => {
                HttpResponse::InternalServerError().json(json!({ "error": msg }))
            }
            AppError::Upstream(msg) => HttpResponse::BadGateway().json(json!({ "error": msg })),
            AppError::SerdeJsonError(err) => {
                HttpResponse::InternalServerError().json(json!({ "error": err.to_string() }))
            }
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
