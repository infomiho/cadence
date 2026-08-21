use std::{sync::OnceLock, time::Duration};

use anyhow::{Result, bail, ensure};
use futures::{FutureExt as _, StreamExt as _, future::BoxFuture};
use gpui::http_client::{AsyncBody, HttpClient, Inner, Response, Url, http};

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
const MAX_IMAGE_DOWNLOAD_BYTES: usize = 10 * 1024 * 1024;

pub(super) struct ImageHttpClient {
    client: reqwest::Client,
    user_agent: http::HeaderValue,
    runtime: tokio::runtime::Handle,
}

impl ImageHttpClient {
    pub(super) fn new() -> Result<Self> {
        let user_agent = http::HeaderValue::from_static("Cadence/0.1");
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .user_agent(user_agent.clone())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()?;
        let runtime = tokio::runtime::Handle::try_current().unwrap_or_else(|_| {
            RUNTIME
                .get_or_init(|| {
                    tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(1)
                        .enable_all()
                        .build()
                        .expect("could not start image HTTP runtime")
                })
                .handle()
                .clone()
        });
        Ok(Self {
            client,
            user_agent,
            runtime,
        })
    }
}

impl HttpClient for ImageHttpClient {
    fn user_agent(&self) -> Option<&http::HeaderValue> {
        Some(&self.user_agent)
    }

    fn send(
        &self,
        request: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>>> {
        let (parts, body) = request.into_parts();
        let mut request = self.client.request(parts.method, parts.uri.to_string());
        request = request.headers(parts.headers);
        request = match body.0 {
            Inner::Empty => request,
            Inner::Bytes(bytes) => request.body(bytes.into_inner()),
            Inner::AsyncReader(_) => {
                return async { bail!("streaming request bodies are unsupported") }.boxed();
            }
        };
        let runtime = self.runtime.clone();
        async move {
            let response = runtime
                .spawn(async move { request.send().await?.error_for_status() })
                .await??;
            let status = response.status();
            let headers = response.headers().clone();
            if let Some(content_length) = response.content_length() {
                ensure!(
                    content_length <= MAX_IMAGE_DOWNLOAD_BYTES as u64,
                    "image response exceeds {MAX_IMAGE_DOWNLOAD_BYTES} byte limit"
                );
            }
            let mut body = Vec::with_capacity(
                response
                    .content_length()
                    .and_then(|length| usize::try_from(length).ok())
                    .unwrap_or_default()
                    .min(MAX_IMAGE_DOWNLOAD_BYTES),
            );
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                ensure!(
                    body.len().saturating_add(chunk.len()) <= MAX_IMAGE_DOWNLOAD_BYTES,
                    "image response exceeds {MAX_IMAGE_DOWNLOAD_BYTES} byte limit"
                );
                body.extend_from_slice(&chunk);
            }
            let mut response = http::Response::builder().status(status);
            *response
                .headers_mut()
                .expect("response builder has headers") = headers;
            Ok(response.body(body.into())?)
        }
        .boxed()
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }
}
