use std::pin::Pin;
use actix_web::web::Bytes;
use futures::{Stream, StreamExt};
use reqwest::Client;
use tokio::time::Duration;
use tracing::{error, info};

use crate::{
    config::ProxyConfig,
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct StreamManager {
    client: Client,
    config: ProxyConfig,
}

impl StreamManager {
    pub fn new(config: ProxyConfig) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(config.connect_timeout))
            .timeout(Duration::from_secs(config.stream_timeout))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if config.follow_redirects {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    pub async fn create_stream(
        &self,
        url: String,
        headers: reqwest::header::HeaderMap,
    ) -> AppResult<(reqwest::header::HeaderMap, impl Stream<Item = Result<Bytes, AppError>>)> {
        let response = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| AppError::Proxy(format!("Failed to connect to upstream: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Upstream(format!(
                "Upstream returned error status: {}",
                response.status()
            )));
        }

        let response_headers = response.headers().clone();
        let stream = response
            .bytes_stream()
            .map(|result| {
                result.map_err(|e| AppError::Proxy(format!("Stream error: {}", e)))
            });

        Ok((response_headers, stream))
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

        Box::pin(stream.map(move |chunk| {
            match chunk {
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