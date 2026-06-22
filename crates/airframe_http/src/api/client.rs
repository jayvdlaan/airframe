//! HTTP API module: traits and error types for building clients independent of runtime.
//! See also: `clients::reqwest` for a concrete implementation, and `api::spec_client` for a spec-driven facade.

use bytes::Bytes;
use http::{Request, Response};
use std::future::Future;

/// A runtime-agnostic HTTP client capability.
#[allow(clippy::type_complexity)]
pub trait HttpClient: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn call(
        &self,
        req: Request<Bytes>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Response<Bytes>, Self::Error>> + Send>>;
}

/// Error type for invoking spec-driven requests.
#[derive(Debug)]
pub enum InvokeError<E: std::error::Error + Send + Sync + 'static> {
    Build(anyhow::Error),
    Client(E),
}

impl<E: std::error::Error + Send + Sync + 'static> std::fmt::Display for InvokeError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Build(e) => write!(f, "request build error: {e}"),
            Self::Client(e) => write!(f, "client error: {e}"),
        }
    }
}
impl<E: std::error::Error + Send + Sync + 'static> std::error::Error for InvokeError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Build(e) => Some(e.as_ref()),
            Self::Client(e) => Some(e),
        }
    }
}

impl<E: std::error::Error + Send + Sync + 'static> From<E> for InvokeError<E> {
    fn from(e: E) -> Self {
        Self::Client(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[derive(Debug)]
    struct DummyErr(&'static str);
    impl std::fmt::Display for DummyErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for DummyErr {}

    #[test]
    fn display_and_source_for_invoke_error() {
        let build_err: InvokeError<DummyErr> = InvokeError::Build(anyhow::anyhow!("oops"));
        assert!(format!("{}", build_err).contains("request build error"));
        assert!(Error::source(&build_err).is_some());

        let client_err: InvokeError<DummyErr> = InvokeError::Client(DummyErr("boom"));
        assert!(format!("{}", client_err).contains("client error"));
        assert!(Error::source(&client_err).is_some());
    }

    #[test]
    fn from_maps_to_client_variant() {
        let e = DummyErr("x");
        let inv: InvokeError<DummyErr> = e.into();
        match inv {
            InvokeError::Client(_) => {}
            _ => panic!("expected client"),
        }
    }
}
