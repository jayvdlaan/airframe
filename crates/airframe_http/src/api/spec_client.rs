//! API module: a spec-driven client facade that composes an `HttpClient` with an `ApiSpec`.
//! This stays independent of concrete HTTP backends.

use bytes::Bytes;
use http::Response;

use crate::api::client::{HttpClient, InvokeError};

/// Spec-driven client facade that composes an HttpClient with an ApiSpec.
pub struct SpecClient<C, S> {
    pub client: C,
    pub spec: S,
}

impl<C, S> SpecClient<C, S> {
    pub fn new(client: C, spec: S) -> Self {
        Self { client, spec }
    }
}

impl<C, S> SpecClient<C, S>
where
    C: HttpClient,
    S: airframe_api::ApiSpec,
{
    /// Build and invoke a request identified by `operation_id` with `params` and optional raw body.
    pub async fn invoke(
        &self,
        operation_id: &str,
        params: &serde_json::Value,
        body: Option<Bytes>,
    ) -> Result<Response<Bytes>, InvokeError<C::Error>> {
        let req = self
            .spec
            .build_request(operation_id, params, body)
            .map_err(InvokeError::Build)?;
        self.client.call(req).await.map_err(InvokeError::Client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Method, Response, StatusCode};
    use std::sync::{Arc, Mutex};

    // Mock HttpClient that records last request and returns a canned response
    #[derive(Clone, Debug)]
    struct DummyErr;
    impl std::fmt::Display for DummyErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "dummy")
        }
    }
    impl std::error::Error for DummyErr {}

    type RequestLog = Arc<Mutex<Option<(http::Method, String, usize)>>>;

    #[derive(Clone)]
    struct MockClient {
        last: RequestLog,
        resp: Response<Bytes>,
    }
    impl MockClient {
        fn new(resp: Response<Bytes>) -> (Self, RequestLog) {
            let last = Arc::new(Mutex::new(None));
            (
                Self {
                    last: last.clone(),
                    resp,
                },
                last,
            )
        }
    }
    impl HttpClient for MockClient {
        type Error = DummyErr;
        fn call(
            &self,
            req: http::Request<Bytes>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Response<Bytes>, Self::Error>> + Send>,
        > {
            let method = req.method().clone();
            let uri = req.uri().to_string();
            let body_len = req.body().len();
            if let Ok(mut guard) = self.last.lock() {
                *guard = Some((method, uri, body_len));
            }
            let resp = self.resp.clone();
            Box::pin(async move { Ok(resp) })
        }
    }

    #[tokio::test]
    async fn invoke_success_passes_through_request_and_response() {
        let spec = airframe_api::CodeSpec::new("http://localhost:8080".parse().unwrap()).route(
            "get_thing",
            Method::GET,
            "/v1/things/{id}",
        );
        let (client, last) = MockClient::new(
            Response::builder()
                .status(StatusCode::OK)
                .body(Bytes::from_static(b"OK"))
                .unwrap(),
        );
        let sc = SpecClient::new(client.clone(), spec);
        let params = serde_json::json!({ "id": 123, "query": {"a": "b"}});
        let resp = sc.invoke("get_thing", &params, None).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.body(), &Bytes::from_static(b"OK"));

        let captured = last.lock().unwrap().clone().unwrap();
        assert_eq!(captured.0, Method::GET);
        assert!(captured
            .1
            .starts_with("http://localhost:8080/v1/things/123"));
        assert!(captured.1.contains("a=b"));
        assert_eq!(captured.2, 0);
    }

    // Spec that forces a build error
    struct BuildErrSpec;
    impl airframe_api::ApiSpec for BuildErrSpec {
        fn base_uri(&self) -> http::Uri {
            "http://example".parse().unwrap()
        }
        fn endpoint(&self, _operation_id: &str) -> Option<airframe_api::Endpoint> {
            None
        }
        fn build_request(
            &self,
            _operation_id: &str,
            _params: &serde_json::Value,
            _body: Option<Bytes>,
        ) -> anyhow::Result<http::Request<Bytes>> {
            Err(anyhow::anyhow!("bad spec"))
        }
    }

    // Reuse DummyErr defined above

    #[derive(Clone)]
    struct ErrClient;
    impl HttpClient for ErrClient {
        type Error = DummyErr;
        fn call(
            &self,
            _req: http::Request<Bytes>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Response<Bytes>, Self::Error>> + Send>,
        > {
            Box::pin(async { Err(DummyErr) })
        }
    }

    #[tokio::test]
    async fn invoke_build_error_is_mapped() {
        let sc = SpecClient::new(ErrClient, BuildErrSpec);
        let err = sc
            .invoke("x", &serde_json::json!({}), None)
            .await
            .err()
            .unwrap();
        match err {
            InvokeError::Build(e) => assert!(format!("{}", e).contains("bad spec")),
            _ => panic!("expected build error"),
        }
    }

    #[tokio::test]
    async fn invoke_client_error_is_mapped() {
        // Use a trivial spec that returns a request
        let spec = airframe_api::CodeSpec::new("http://localhost".parse().unwrap()).route(
            "op",
            Method::GET,
            "/ping",
        );
        let sc = SpecClient::new(ErrClient, spec);
        let err = sc
            .invoke("op", &serde_json::json!({}), None)
            .await
            .err()
            .unwrap();
        match err {
            InvokeError::Client(_e) => { /* expected */ }
            _ => panic!("expected client error"),
        }
    }
}
