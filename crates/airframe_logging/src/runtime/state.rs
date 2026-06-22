#![cfg_attr(not(feature = "module"), allow(dead_code))]
//! Logging runtime state and control.

use std::sync::{Arc, Mutex};

use crate::api::config::LoggingConfig;
use crate::layer_parent::ParentSubscriber;
use crate::layers::sinks_layer::SinksLayer;

#[derive(Debug, Clone)]
pub struct LoggingState {
    inner: Arc<Mutex<LoggingConfig>>,
}
impl LoggingState {
    pub fn new(cfg: LoggingConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(cfg)),
        }
    }
    pub fn get(&self) -> LoggingConfig {
        self.inner.lock().unwrap().clone()
    }
    pub fn set(&self, cfg: LoggingConfig) {
        *self.inner.lock().unwrap() = cfg;
    }
}

#[derive(Clone)]
pub struct LoggingControl {
    pub(crate) filter_handle: tracing_subscriber::reload::Handle<
        tracing_subscriber::EnvFilter,
        tracing_subscriber::Registry,
    >,
    pub(crate) sinks_handle: tracing_subscriber::reload::Handle<SinksLayer, ParentSubscriber>,
    pub(crate) file_guards:
        std::sync::Arc<std::sync::Mutex<Vec<tracing_appender::non_blocking::WorkerGuard>>>,
}

impl LoggingControl {
    pub fn set_filter(&self, filter: tracing_subscriber::EnvFilter) {
        let _ = self.filter_handle.reload(filter);
    }
    pub(crate) fn set_sinks(
        &self,
        layer: SinksLayer,
        guards: Vec<tracing_appender::non_blocking::WorkerGuard>,
    ) {
        // swap guards to ensure old writers are dropped, new ones kept alive
        let mut g = self.file_guards.lock().unwrap();
        *g = guards;
        let _ = self.sinks_handle.reload(layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn logging_state_get_set_roundtrip() {
        let s = LoggingState::new(LoggingConfig::default());
        let mut cfg = s.get();
        assert_eq!(cfg.directives, None);
        cfg.directives = Some(vec!["debug".into()]);
        s.set(cfg.clone());
        assert_eq!(s.get(), cfg);
    }

    #[test]
    fn logging_control_set_filter_and_sinks_swaps_guards() {
        // Build reloadable EnvFilter and SinksLayer handles
        let (filter_layer, filter_handle) =
            tracing_subscriber::reload::Layer::new(tracing_subscriber::EnvFilter::new("info"));
        let empty_sinks = SinksLayer { inner: Vec::new() };
        let (sinks_layer, sinks_handle) = tracing_subscriber::reload::Layer::new(empty_sinks);

        // Compose a temporary subscriber so handles are valid
        let subscriber = tracing_subscriber::registry::Registry::default()
            .with(filter_layer)
            .with(sinks_layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let control = LoggingControl {
            filter_handle,
            sinks_handle,
            file_guards: Arc::new(Mutex::new(Vec::new())),
        };

        // set_filter should not panic
        control.set_filter(tracing_subscriber::EnvFilter::new("debug"));

        // Prepare a new sinks layer and a dummy guard, then set_sinks and ensure the guard is retained
        let new_layer = SinksLayer { inner: Vec::new() };
        // Create a WorkerGuard using a non-blocking writer over a sink()
        let builder = tracing_appender::non_blocking::NonBlockingBuilder::default();
        let (_nb, guard) = builder.finish(std::io::sink());
        control.set_sinks(new_layer, vec![guard]);
        let len = control.file_guards.lock().unwrap().len();
        assert_eq!(len, 1);
    }
}
