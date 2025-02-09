use actix_web::{
    body::SizedStream,
    web::{self, Bytes},
    HttpRequest, HttpResponse,
};
use futures::stream;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::boxed::Box;
use std::str::FromStr;

use crate::{
    auth::{encryption::ProxyData, EncryptionHandler},
    error::{AppError, AppResult},
    models::request::{GenerateUrlRequest, SUPPORTED_REQUEST_HEADERS, SUPPORTED_RESPONSE_HEADERS},
    proxy::stream::{ResponseStream, StreamManager},
};

async fn handle_proxy_request(
    req: HttpRequest,
    stream_manager: web::Data<StreamManager>,
    proxy_data: web::ReqData<ProxyData>,
    is_head: bool,
) -> AppResult<HttpResponse> {
    // Prepare headers
    let mut request_headers = HeaderMap::new();

    // Add supported headers from original request
    for &header_name in SUPPORTED_REQUEST_HEADERS {
        if let Some(value) = req.headers().get(header_name) {
            request_headers.insert(
                HeaderName::from_str(header_name)
                    .map_err(|e| AppError::Internal(format!("Invalid header name: {}", e)))?,
                HeaderValue::try_from(value.as_bytes())
                    .map_err(|e| AppError::Internal(format!("Invalid header value: {}", e)))?,
            );
        }
    }

    // Add custom headers from proxy data
    if let Some(custom_headers) = &proxy_data.request_headers {
        for (key, value) in custom_headers
            .as_object()
            .unwrap_or(&serde_json::Map::new())
        {
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

    tracing::debug!("Request headers: {:?}", request_headers);

    // Create the stream
    let (upstream_headers, stream_opt) = stream_manager
        .create_stream(proxy_data.destination.clone(), request_headers, is_head)
        .await?;

    tracing::debug!("Upstream headers: {:?}", upstream_headers);

    // Prepare response headers
    let mut response = HttpResponse::Ok();

    // Add supported headers from upstream response
    for &header_name in SUPPORTED_RESPONSE_HEADERS {
        if let Some(value) = upstream_headers.get(header_name) {
            if let Ok(converted_value) =
                actix_web::http::header::HeaderValue::from_str(value.to_str().unwrap_or_default())
            {
                response.insert_header((header_name, converted_value));
            }
        }
    }

    // Get content length from headers
    let content_length = upstream_headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    // Add custom response headers from proxy data
    if let Some(custom_headers) = &proxy_data.response_headers {
        for (key, value) in custom_headers
            .as_object()
            .unwrap_or(&serde_json::Map::new())
        {
            if let Some(value_str) = value.as_str() {
                response.insert_header((
                    actix_web::http::header::HeaderName::from_str(key)
                        .map_err(|e| AppError::Internal(format!("Invalid header name: {}", e)))?,
                    actix_web::http::header::HeaderValue::from_str(value_str)
                        .map_err(|e| AppError::Internal(format!("Invalid header value: {}", e)))?,
                ));
            }
        }
    }

    if is_head {
        // For HEAD requests, use empty stream
        let empty_stream = Box::pin(stream::empty::<Result<Bytes, std::io::Error>>());
        Ok(response
            .no_chunking(content_length)
            .body(SizedStream::new(content_length, empty_stream)))
    } else if let Some(stream) = stream_opt {
        let stream_with_progress = stream_manager.stream_with_progress(stream).await;
        let response_stream = ResponseStream::new(stream_with_progress);
        // If we have a content length, use SizedStream
        if content_length > 0 {
            Ok(response
                .no_chunking(content_length)
                .body(SizedStream::new(content_length, response_stream)))
        } else {
            // Fall back to chunked transfer encoding if no content length
            Ok(response.streaming(response_stream))
        }
    } else {
        Err(AppError::Internal("Stream not available".to_string()))
    }
}

pub async fn proxy_stream_get(
    req: HttpRequest,
    stream_manager: web::Data<StreamManager>,
    proxy_data: web::ReqData<ProxyData>,
) -> AppResult<HttpResponse> {
    handle_proxy_request(req, stream_manager, proxy_data, false).await
}

pub async fn proxy_stream_head(
    req: HttpRequest,
    stream_manager: web::Data<StreamManager>,
    proxy_data: web::ReqData<ProxyData>,
) -> AppResult<HttpResponse> {
    handle_proxy_request(req, stream_manager, proxy_data, true).await
}

pub async fn generate_url(req: web::Json<GenerateUrlRequest>) -> AppResult<HttpResponse> {
    let mut url = req.mediaflow_proxy_url.clone();

    if let Some(endpoint) = &req.endpoint {
        url = format!(
            "{}/{}",
            url.trim_end_matches('/'),
            endpoint.trim_start_matches('/')
        );
    }

    // If api_password is provided in the request body, encrypt the data
    if let Some(api_password) = &req.api_password {
        let encryption_handler = EncryptionHandler::new(api_password.as_bytes()).map_err(|e| {
            AppError::Internal(format!("Failed to create encryption handler: {}", e))
        })?;

        let proxy_data = ProxyData {
            destination: req.destination_url.clone(),
            query_params: Some(
                serde_json::to_value(&req.query_params).map_err(AppError::SerdeJsonError)?,
            ),
            request_headers: Some(
                serde_json::to_value(&req.request_headers).map_err(AppError::SerdeJsonError)?,
            ),
            response_headers: Some(
                serde_json::to_value(&req.response_headers).map_err(AppError::SerdeJsonError)?,
            ),
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
        url = format!("{}?token={}", url, token);
    } else {
        // If no api_password in body, encode parameters in URL
        let mut params = req.query_params.clone();
        params.insert("d".to_string(), req.destination_url.clone());

        // Add headers if provided with proper prefixes
        for (key, value) in &req.request_headers {
            params.insert(format!("h_{}", key), value.clone());
        }
        for (key, value) in &req.response_headers {
            params.insert(format!("r_{}", key), value.clone());
        }

        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(&v.to_string())))
            .collect::<Vec<_>>()
            .join("&");

        url = format!("{}?{}", url, query_string);
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "url": url
    })))
}

pub async fn get_public_ip(stream_manager: web::Data<StreamManager>) -> AppResult<HttpResponse> {
    let response = stream_manager
        .make_request(
            "https://api.ipify.org?format=json".to_string(),
            HeaderMap::new(),
        )
        .await?;

    let ip_data = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse IP response: {}", e)))?;

    Ok(HttpResponse::Ok().json(ip_data))
}
