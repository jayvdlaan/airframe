//! Config listener registry broadcasts raw config to subscribers on reloads.

use std::sync::{Arc, RwLock};

/// Listener for configuration reload events.
/// Implementations should be cheap and non-blocking; heavy work should be offloaded.
pub trait ConfigListener: Send + Sync {
    fn on_config_reload(&self, raw: &toml::Value);
}

/// Registry of `ConfigListener`s in insertion order.
#[derive(Default)]
pub struct ConfigListenerRegistry {
    inner: RwLock<Vec<Arc<dyn ConfigListener>>>,
}
impl ConfigListenerRegistry {
    pub fn add(&self, l: Arc<dyn ConfigListener>) {
        self.inner.write().unwrap().push(l);
    }
    pub fn all(&self) -> Vec<Arc<dyn ConfigListener>> {
        self.inner.read().unwrap().clone()
    }
}

/// Helper to get or create the ConfigListenerRegistry in the ServiceRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_config_listener_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<ConfigListenerRegistry> {
    if let Some(r) = svcs.get::<ConfigListenerRegistry>() {
        return r;
    }
    let reg = Arc::new(ConfigListenerRegistry::default());
    svcs.register::<ConfigListenerRegistry>(reg.clone());
    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct TestListener {
        call_count: AtomicU32,
    }
    impl TestListener {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }
        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }
    impl ConfigListener for TestListener {
        fn on_config_reload(&self, _raw: &toml::Value) {
            self.call_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = ConfigListenerRegistry::default();
        assert!(reg.all().is_empty());
    }

    #[test]
    fn registry_add_and_all() {
        let reg = ConfigListenerRegistry::default();
        let l1 = Arc::new(TestListener::new());
        let l2 = Arc::new(TestListener::new());

        reg.add(l1.clone());
        reg.add(l2.clone());

        let all = reg.all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn listener_receives_config() {
        let listener = Arc::new(TestListener::new());
        let val = toml::Value::Table(toml::map::Map::new());
        listener.on_config_reload(&val);
        assert_eq!(listener.calls(), 1);
    }
}
