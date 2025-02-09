use actix_web::HttpMessage;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use futures::future::LocalBoxFuture;
use serde_json::Value;
use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::Arc;

use crate::auth::encryption::{EncryptionHandler, ProxyData};
use crate::error::AppError;

const OPEN_ENDPOINTS: &[&str] = &["/proxy/generate_url", "/health"];

#[derive(Clone)]
pub struct AuthMiddleware {
    encryption_handler: Option<Arc<EncryptionHandler>>,
    api_password: String,
}

impl AuthMiddleware {
    pub fn new(api_password: String) -> Self {
        let encryption_handler = if !api_password.is_empty() {
            Some(Arc::new(
                EncryptionHandler::new(api_password.as_bytes())
                    .expect("Failed to create encryption handler"),
            ))
        } else {
            None
        };

        Self {
            encryption_handler,
            api_password,
        }
    }

    fn extract_query_params(query_string: &str) -> serde_json::Map<String, Value> {
        let mut params = serde_json::Map::new();
        for pair in query_string.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                if !key.is_empty() && !value.is_empty() {
                    params.insert(
                        key.to_string(),
                        Value::String(
                            urlencoding::decode(value)
                                .unwrap_or_else(|_| value.into())
                                .into_owned(),
                        ),
                    );
                }
            }
        }
        params
    }
}

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
            encryption_handler: self.encryption_handler.clone(),
            api_password: self.api_password.clone(),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
    encryption_handler: Option<Arc<EncryptionHandler>>,
    api_password: String,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let encryption_handler = self.encryption_handler.clone();
        let api_password = self.api_password.clone();

        Box::pin(async move {
            // Check if path is in open endpoints
            if OPEN_ENDPOINTS.iter().any(|path| req.path() == *path) {
                return service.call(req).await;
            }

            // If API password is not set, allow all requests
            if api_password.is_empty() {
                return service.call(req).await;
            }

            let query_string = req.query_string().to_owned();
            let query_params = AuthMiddleware::extract_query_params(&query_string);

            // Check for encrypted token
            if let Some(token) = query_params.get("token").and_then(|v| v.as_str()) {
                if let Some(handler) = encryption_handler {
                    // Get client IP if needed for validation
                    let client_ip = req
                        .connection_info()
                        .realip_remote_addr()
                        .map(|s| s.to_string());

                    // Decrypt and validate token
                    let proxy_data = handler
                        .decrypt(token, client_ip.as_deref())
                        .map_err(Error::from)?;

                    // validate api password
                    if proxy_data
                        .query_params
                        .as_ref()
                        .and_then(|v| v.get("api_password"))
                        .and_then(|v| v.as_str())
                        != Some(&api_password)
                    {
                        return Err(AppError::Auth(
                            "Invalid or missing authentication".to_string(),
                        )
                        .into());
                    }

                    // Store proxy data in request extensions
                    req.extensions_mut().insert(proxy_data);
                    return service.call(req).await;
                }
            }

            // Check for direct API password
            if let Some(password) = query_params.get("api_password").and_then(|v| v.as_str()) {
                if password == api_password {
                    if let Some(destination) = query_params.get("d").and_then(|v| v.as_str()) {
                        // Create proxy data from query parameters
                        let proxy_data = ProxyData {
                            destination: destination.to_string(),
                            query_params: Some(Value::Object(query_params.clone())),
                            request_headers: Some(Value::Object(
                                query_params
                                    .iter()
                                    .filter_map(|(k, v)| {
                                        k.strip_prefix("h_")
                                            .map(|stripped| (stripped.to_string(), v.clone()))
                                    })
                                    .collect(),
                            )),
                            response_headers: Some(Value::Object(
                                query_params
                                    .iter()
                                    .filter_map(|(k, v)| {
                                        k.strip_prefix("r_")
                                            .map(|stripped| (stripped.to_string(), v.clone()))
                                    })
                                    .collect(),
                            )),
                            exp: None,
                            ip: None,
                        };

                        // Store proxy data in request extensions
                        req.extensions_mut().insert(proxy_data);
                        return service.call(req).await;
                    }
                    return service.call(req).await;
                }
            }

            Err(AppError::Auth("Invalid or missing authentication".to_string()).into())
        })
    }
}
