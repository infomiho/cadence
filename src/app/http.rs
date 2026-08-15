use std::{any::type_name, sync::OnceLock};

use anyhow::{Result, bail};
use futures::{FutureExt as _, future::BoxFuture};
use gpui_http_client::{AsyncBody, HttpClient, Inner, Response, Url, http};

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

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
    fn type_name(&self) -> &'static str {
        type_name::<Self>()
    }

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
            let response = runtime.spawn(async move { request.send().await }).await??;
            let status = response.status();
            let headers = response.headers().clone();
            let body = response.bytes().await?;
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
