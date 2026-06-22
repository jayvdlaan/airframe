//! Config defaults contributor registry.
//! This allows modules to inject baseline TOML config that is merged at the
//! lowest precedence before files/env/cli. Later contributors override earlier ones.

use std::sync::{Arc, RwLock};

/// Trait for modules to contribute default TOML config.
/// Return a toml::Value (usually a Table) to be merged at lowest precedence.
pub trait ConfigDefaultsContributor: Send + Sync {
    fn defaults(&self) -> toml::Value;
}

/// A registry capability storing all defaults contributors in insertion order.
#[derive(Default)]
pub struct ConfigDefaultsRegistry {
    inner: RwLock<Vec<Arc<dyn ConfigDefaultsContributor>>>,
}
impl ConfigDefaultsRegistry {
    pub fn add(&self, c: Arc<dyn ConfigDefaultsContributor>) {
        self.inner.write().unwrap().push(c);
    }
    pub fn all(&self) -> Vec<Arc<dyn ConfigDefaultsContributor>> {
        self.inner.read().unwrap().clone()
    }
}

/// Helper to get or create the ConfigDefaultsRegistry in the ServiceRegistry.
#[cfg(feature = "module")]
pub fn get_or_create_config_defaults_registry(
    svcs: &airframe_core::registry::ServiceRegistry,
) -> Arc<ConfigDefaultsRegistry> {
    if let Some(r) = svcs.get::<ConfigDefaultsRegistry>() {
        return r;
    }
    let reg = Arc::new(ConfigDefaultsRegistry::default());
    svcs.register::<ConfigDefaultsRegistry>(reg.clone());
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestContributor {
        defaults: toml::Value,
    }
    impl TestContributor {
        fn new(defaults: toml::Value) -> Self {
            Self { defaults }
        }
    }
    impl ConfigDefaultsContributor for TestContributor {
        fn defaults(&self) -> toml::Value {
            self.defaults.clone()
        }
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = ConfigDefaultsRegistry::default();
        assert!(reg.all().is_empty());
    }

    #[test]
    fn registry_add_and_all() {
        let reg = ConfigDefaultsRegistry::default();
        let c1 = Arc::new(TestContributor::new(toml::Value::Boolean(true)));
        let c2 = Arc::new(TestContributor::new(toml::Value::Boolean(false)));

        reg.add(c1.clone());
        reg.add(c2.clone());

        let all = reg.all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn contributor_returns_defaults() {
        let table = toml::Value::Table(toml::toml! {
            [server]
            port = 8080
        });
        let c = TestContributor::new(table.clone());
        let defaults = c.defaults();
        assert_eq!(defaults, table);
    }

    #[test]
    fn contributors_preserve_insertion_order() {
        let reg = ConfigDefaultsRegistry::default();
        let c1 = Arc::new(TestContributor::new(toml::Value::Integer(1)));
        let c2 = Arc::new(TestContributor::new(toml::Value::Integer(2)));
        let c3 = Arc::new(TestContributor::new(toml::Value::Integer(3)));

        reg.add(c1);
        reg.add(c2);
        reg.add(c3);

        let all = reg.all();
        assert_eq!(all[0].defaults(), toml::Value::Integer(1));
        assert_eq!(all[1].defaults(), toml::Value::Integer(2));
        assert_eq!(all[2].defaults(), toml::Value::Integer(3));
    }
}
