use actix_web::web::Bytes;
use futures::{Stream, StreamExt};
use reqwest::{Client, Proxy, Response};
use std::pin::Pin;
use tokio::time::{timeout, Duration};
use tracing::{error, info};

use crate::{
    config::{ProxyConfig, ProxyRouter},
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct StreamManager {
    client: Client,
    config: ProxyConfig,
    proxy_router: ProxyRouter,
}

impl StreamManager {
    pub fn new(config: ProxyConfig) -> Self {
        let proxy_router = ProxyRouter::from_config(&config);
        let client = Self::create_client(&config, &proxy_router);

        Self {
            client,
            config,
            proxy_router,
        }
    }

    fn create_client(config: &ProxyConfig, proxy_router: &ProxyRouter) -> Client {
        let follow_redirects = config.follow_redirects;
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(config.connect_timeout))
            // Remove the overall timeout to prevent stream interruption
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(0) // Disable connection pooling
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if follow_redirects {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }));

        if let Some(default_proxy) = proxy_router.default_proxy() {
            if let Ok(proxy) = Proxy::all(default_proxy) {
                builder = builder.proxy(proxy);
            }
        }

        builder.build().expect("Failed to create HTTP client")
    }

    pub async fn make_request(
        &self,
        url: String,
        headers: reqwest::header::HeaderMap,
    ) -> AppResult<Response> {
        let proxy_config = self.proxy_router.get_proxy_config(&url);

        let client = if let Some(config) = proxy_config {
            let mut builder = Client::builder()
                .connect_timeout(Duration::from_secs(self.config.connect_timeout))
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(0);

            if config.proxy {
                if let Some(proxy_url) = config.proxy_url.as_ref() {
                    match Proxy::all(proxy_url) {
                        Ok(proxy) => {
                            info!("Using proxy {} for {}", proxy_url, url);
                            builder = builder.proxy(proxy);
                        }
                        Err(e) => {
                            error!("Failed to create proxy for {}: {}", proxy_url, e);
                            return Err(AppError::Internal(format!(
                                "Failed to create proxy: {}",
                                e
                            )));
                        }
                    }
                }
            }

            if !config.verify_ssl {
                tracing::warn!("SSL verification disabled for {}", url);
                builder = builder.danger_accept_invalid_certs(true);
            }

            builder
                .build()
                .map_err(|e| AppError::Internal(format!("Failed to create client: {}", e)))?
        } else {
            self.client.clone()
        };

        let response = timeout(
            Duration::from_secs(self.config.connect_timeout),
            client.get(&url).headers(headers).send(),
        )
        .await
        .map_err(|e| AppError::Proxy(format!("Connection timeout: {}", e)))?
        .map_err(|e| AppError::Proxy(format!("Failed to connect to upstream: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Upstream(format!(
                "Upstream returned error status: {}",
                response.status()
            )));
        }

        Ok(response)
    }

    pub async fn create_stream(
        &self,
        url: String,
        headers: reqwest::header::HeaderMap,
        is_head: bool,
    ) -> AppResult<(
        reqwest::header::HeaderMap,
        Option<impl Stream<Item = Result<Bytes, AppError>>>,
    )> {
        // Always make a GET request but don't read the body for HEAD
        let response = self.make_request(url, headers).await?;
        let response_headers = response.headers().clone();

        if is_head {
            // For HEAD requests, return only headers
            Ok((response_headers, None))
        } else {
            // For GET requests, return headers and stream
            let stream = response
                .bytes_stream()
                .map(|result| result.map_err(|e| AppError::Proxy(format!("Stream error: {}", e))));

            Ok((response_headers, Some(stream)))
        }
    }

    pub async fn stream_with_progress<S>(
        &self,
        stream: S,
    ) -> impl Stream<Item = Result<Bytes, AppError>>
    where
        S: Stream<Item = Result<Bytes, AppError>> + 'static,
    {
        let buffer_size = self.config.buffer_size;
        let mut total_bytes = 0usize;

        Box::pin(stream.map(move |chunk| match chunk {
            Ok(bytes) => {
                total_bytes += bytes.len();
                if total_bytes % (buffer_size * 10) == 0 {
                    info!("Streamed {} bytes", total_bytes);
                }
                Ok(bytes)
            }
            Err(e) => {
                error!("Streaming error after {} bytes: {}", total_bytes, e);
                Err(e)
            }
        }))
    }
}

pub struct ResponseStream<S> {
    inner: Pin<Box<S>>,
}

impl<S> ResponseStream<S>
where
    S: Stream<Item = Result<Bytes, AppError>>,
{
    pub fn new(stream: S) -> Self {
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl<S> Stream for ResponseStream<S>
where
    S: Stream<Item = Result<Bytes, AppError>>,
{
    type Item = Result<Bytes, AppError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
