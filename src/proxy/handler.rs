use actix_web::{
    web::{self},
    HttpRequest, HttpResponse,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::str::FromStr;

use crate::{
    auth::encryption::ProxyData,
    error::{AppError, AppResult},
    models::request::{SUPPORTED_REQUEST_HEADERS, SUPPORTED_RESPONSE_HEADERS, GenerateUrlRequest},
    proxy::stream::{ResponseStream, StreamManager},
};

pub async fn proxy_stream(
    req: HttpRequest,
    stream_manager: web::Data<StreamManager>,
    proxy_data: web::ReqData<ProxyData>,
) -> AppResult<HttpResponse> {
    // Prepare headers
    let mut request_headers = HeaderMap::new();
    
    // Add supported headers from the original request
    for &header_name in SUPPORTED_REQUEST_HEADERS {
        if let Some(value) = req.headers().get(header_name) {
            request_headers.insert(
                HeaderName::from_str(header_name)
                    .map_err(|e| AppError::Internal(format!("Invalid header name: {}", e)))?,
                value.clone(),
            );
        }
    }

    // Add custom headers from proxy data
    if let Some(custom_headers) = &proxy_data.request_headers {
        for (key, value) in custom_headers.as_object().unwrap_or(&serde_json::Map::new()) {
            if let Some(value_str) = value.as_str() {
                request_headers.insert(
                    HeaderName::from_str(key)
                        .map_err(|e| AppError::Internal(format!("Invalid header name: {}", e)))?,
                    HeaderValue::from_str(value_str)
                        .map_err(|e| AppError::Internal(format!("Invalid header value: {}", e)))?,
                );
            }
        }
    }

    // Create the stream
    let (upstream_headers, stream) = stream_manager
        .create_stream(proxy_data.destination.clone(), request_headers)
        .await?;

    // Prepare response headers
    let mut response_headers = HeaderMap::new();

    // Add supported headers from upstream response
    for &header_name in SUPPORTED_RESPONSE_HEADERS {
        if let Some(value) = upstream_headers.get(header_name) {
            response_headers.insert(
                HeaderName::from_str(header_name)
                    .map_err(|e| AppError::Internal(format!("Invalid header name: {}", e)))?,
                value.clone(),
            );
        }
    }

    // Add custom response headers from proxy data
    if let Some(custom_headers) = &proxy_data.response_headers {
        for (key, value) in custom_headers.as_object().unwrap_or(&serde_json::Map::new()) {
            if let Some(value_str) = value.as_str() {
                response_headers.insert(
                    HeaderName::from_str(key)
                        .map_err(|e| AppError::Internal(format!("Invalid header name: {}", e)))?,
                    HeaderValue::from_str(value_str)
                        .map_err(|e| AppError::Internal(format!("Invalid header value: {}", e)))?,
                );
            }
        }
    }

    // Create streaming response
    let stream_with_progress = stream_manager.stream_with_progress(stream).await;
    let response_stream = ResponseStream::new(stream_with_progress);
    
    Ok(HttpResponse::Ok()
        .content_type("application/octet-stream")
        .streaming(response_stream))
}

pub async fn generate_url(
    req: web::Json<GenerateUrlRequest>,
    encryption_handler: web::Data<crate::auth::encryption::EncryptionHandler>,
) -> AppResult<HttpResponse> {
    let proxy_data = ProxyData {
        destination: req.destination_url.clone(),
        query_params: Some(serde_json::to_value(&req.query_params).map_err(AppError::SerdeJsonError)?),
        request_headers: Some(serde_json::to_value(&req.request_headers).map_err(AppError::SerdeJsonError)?),
        response_headers: Some(serde_json::to_value(&req.response_headers).map_err(AppError::SerdeJsonError)?),
        exp: req.expiration.map(|e| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + e
        }),
        ip: req.ip.clone(),
    };

    let token = encryption_handler.encrypt(&proxy_data)?;
    let mut url = req.mediaflow_proxy_url.clone();
    
    if let Some(endpoint) = &req.endpoint {
        url = format!("{}/{}", url.trim_end_matches('/'), endpoint.trim_start_matches('/'));
    }

    url = format!("{}?token={}", url, token);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "encoded_url": url
    })))
}
