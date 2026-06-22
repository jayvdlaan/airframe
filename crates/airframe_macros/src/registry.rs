//! Service registry macros for reducing boilerplate.

/// Register a service in the ServiceRegistry.
///
/// # Examples
///
/// Simple registration:
/// ```ignore
/// use airframe_macros::register_service;
///
/// register_service!(registry, MyService::new());
/// ```
///
/// Register under multiple trait types:
/// ```ignore
/// register_service!(registry, my_service => [dyn MyTrait, dyn OtherTrait]);
/// ```
#[macro_export]
macro_rules! register_service {
    ($registry:expr, $service:expr) => {{
        let svc = ::std::sync::Arc::new($service);
        $registry.register(svc);
    }};
    ($registry:expr, $service:expr => [$($trait:ty),+ $(,)?]) => {{
        let svc = ::std::sync::Arc::new($service);
        $(
            $registry.register::<$trait>(svc.clone());
        )+
    }};
}

/// Define an extension trait for typed service access on ServiceRegistry.
///
/// This macro generates a trait with typed getter methods for commonly-used
/// services, reducing boilerplate when accessing services from the registry.
///
/// # Examples
///
/// ```ignore
/// use airframe_macros::service_ext;
/// use airframe_core::registry::ServiceRegistry;
///
/// trait ConfigProvider: Send + Sync { /* ... */ }
/// trait Logger: Send + Sync { /* ... */ }
///
/// service_ext!(ServiceRegistryExt for ServiceRegistry {
///     fn config(&self) -> Option<Arc<dyn ConfigProvider>>;
///     fn logger(&self) -> Option<Arc<dyn Logger>>;
/// });
///
/// // Usage:
/// let config = registry.config();
/// let logger = registry.logger();
/// ```
#[macro_export]
macro_rules! service_ext {
    ($trait_name:ident for $registry:ty {
        $(fn $fn_name:ident(&self) -> Option<Arc<$ret:ty>>;)*
    }) => {
        pub trait $trait_name {
            $(fn $fn_name(&self) -> Option<::std::sync::Arc<$ret>>;)*
        }

        impl $trait_name for $registry {
            $(fn $fn_name(&self) -> Option<::std::sync::Arc<$ret>> {
                self.get::<$ret>()
            })*
        }
    };
}

#[cfg(test)]
mod tests {
    use airframe_core::registry::ServiceRegistry;
    use std::sync::Arc;

    struct TestService {
        value: i32,
    }

    #[test]
    fn register_service_simple() {
        let registry = ServiceRegistry::default();
        register_service!(registry, TestService { value: 42 });

        let svc = registry.get::<TestService>().expect("service should exist");
        assert_eq!(svc.value, 42);
    }

    trait MyTrait: Send + Sync {
        fn get_value(&self) -> i32;
    }

    trait OtherTrait: Send + Sync {
        fn describe(&self) -> &'static str;
    }

    struct MultiService {
        value: i32,
    }

    impl MyTrait for MultiService {
        fn get_value(&self) -> i32 {
            self.value
        }
    }

    impl OtherTrait for MultiService {
        fn describe(&self) -> &'static str {
            "MultiService"
        }
    }

    #[test]
    fn register_service_multi_trait() {
        let registry = ServiceRegistry::default();
        register_service!(registry, MultiService { value: 100 } => [dyn MyTrait, dyn OtherTrait]);

        let my_svc = registry.get::<dyn MyTrait>().expect("MyTrait should exist");
        assert_eq!(my_svc.get_value(), 100);

        let other_svc = registry
            .get::<dyn OtherTrait>()
            .expect("OtherTrait should exist");
        assert_eq!(other_svc.describe(), "MultiService");
    }

    #[test]
    fn service_ext_macro() {
        trait ConfigProvider: Send + Sync {
            fn get_config(&self) -> &str;
        }

        struct Config;
        impl ConfigProvider for Config {
            fn get_config(&self) -> &str {
                "test_config"
            }
        }

        service_ext!(TestRegistryExt for ServiceRegistry {
            fn config_provider(&self) -> Option<Arc<dyn ConfigProvider>>;
        });

        let registry = ServiceRegistry::default();
        registry.register::<dyn ConfigProvider>(Arc::new(Config));

        // Use the extension trait method
        let config = registry.config_provider().expect("config should exist");
        assert_eq!(config.get_config(), "test_config");
    }
}
