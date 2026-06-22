//!
//! clients::reqwest — Reqwest-backed HttpClient adapter.
//! Enable the `client` feature in `airframe_http` to use this.

use bytes::Bytes;
use http::{Request, Response};
use std::future::Future;

use crate::api::client::HttpClient;

/// A thin HttpClient adapter backed by reqwest::Client.
pub struct ReqwestClient {
    inner: reqwest::Client,
}

impl ReqwestClient {
    pub fn new() -> Self {
        Self {
            inner: reqwest::Client::new(),
        }
    }
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { inner: client }
    }
}

impl HttpClient for ReqwestClient {
    type Error = reqwest::Error;

    fn call(
        &self,
        req: Request<Bytes>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Response<Bytes>, Self::Error>> + Send>> {
        let client = self.inner.clone();
        Box::pin(async move {
            let (parts, body) = req.into_parts();
            let url_str = parts.uri.to_string();
            let mut rb = client.request(parts.method, url_str);
            // propagate headers
            if !parts.headers.is_empty() {
                let mut h = reqwest::header::HeaderMap::new();
                for (k, v) in parts.headers.iter() {
                    h.append(k, v.clone());
                }
                rb = rb.headers(h);
            }
            let resp = rb.body(body).send().await?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let body_bytes = resp.bytes().await?;
            let mut builder = http::Response::builder().status(status);
            for (k, v) in headers.iter() {
                builder = builder.header(k, v);
            }
            let response = builder
                .body(Bytes::from(body_bytes))
                .expect("response build should not fail for valid headers/status");
            Ok(response)
        })
    }
}
