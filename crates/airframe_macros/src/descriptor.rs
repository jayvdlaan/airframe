//! Module descriptor macro for reducing boilerplate.

/// Create a `ModuleDescriptor` with named fields.
///
/// This macro provides a convenient way to construct `ModuleDescriptor` instances
/// with sensible defaults for optional fields.
///
/// # Required Fields
/// - `name`: The module name as a string literal
/// - `version`: The semver version as a string literal (e.g., "1.0.0")
///
/// # Optional Fields
/// - `provides`: Array of capabilities this module provides
/// - `requires`: Array of capabilities this module requires
/// - `optional_requires`: Array of optional capabilities
///
/// Each capability entry is any `&'static str` *expression*. Prefer the typed
/// [`Cap`](airframe_core::module::Cap) constants from `airframe_core::module`
/// via their `.0` accessor (e.g. `CAP_HTTP_SERVER.0`): a misspelled constant is
/// a compile error, whereas a raw string typo silently creates a phantom
/// capability. Raw string literals remain accepted for compatibility.
///
/// # Examples
///
/// Minimal usage:
/// ```ignore
/// use airframe_macros::module_descriptor;
///
/// let desc = module_descriptor!(
///     name: "my_module",
///     version: "1.0.0"
/// );
/// ```
///
/// With capabilities (typed, typo-checked):
/// ```ignore
/// use airframe_core::module::{CAP_HTTP_SERVER, CAP_ROUTER, CAP_CONFIG, CAP_LOGGING, CAP_METRICS};
///
/// let desc = module_descriptor!(
///     name: "http_server",
///     version: "2.1.0",
///     provides: [CAP_HTTP_SERVER.0, CAP_ROUTER.0],
///     requires: [CAP_CONFIG.0, CAP_LOGGING.0],
///     optional_requires: [CAP_METRICS.0],
/// );
/// ```
#[macro_export]
macro_rules! module_descriptor {
    (
        name: $name:literal,
        version: $version:literal
        $(, provides: [$($provides:expr),* $(,)?])?
        $(, requires: [$($requires:expr),* $(,)?])?
        $(, optional_requires: [$($opt:expr),* $(,)?])?
        $(,)?
    ) => {
        $crate::ModuleDescriptor {
            name: $name,
            version: semver::Version::parse($version).expect("invalid semver version"),
            provides: &[$($($provides),*)?],
            requires: &[$($($requires),*)?],
            optional_requires: &[$($($opt),*)?],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        }
    };
}

/// Implement the `Module::descriptor` accessor for a module whose descriptor is
/// stored in a `desc: ModuleDescriptor` field. Use inside an `impl Module` block
/// to avoid hand-copying the identical accessor in every module:
///
/// ```ignore
/// impl Module for MyModule {
///     airframe_macros::impl_descriptor!();
///     async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> { Ok(()) }
/// }
/// ```
#[macro_export]
macro_rules! impl_descriptor {
    () => {
        fn descriptor(&self) -> &$crate::ModuleDescriptor {
            &self.desc
        }
    };
}

#[cfg(test)]
mod tests {
    use airframe_core::module::ModuleDescriptor;

    #[test]
    fn module_descriptor_minimal() {
        let desc: ModuleDescriptor = module_descriptor!(
            name: "test_module",
            version: "0.1.0"
        );
        assert_eq!(desc.name, "test_module");
        assert_eq!(desc.version.major, 0);
        assert_eq!(desc.version.minor, 1);
        assert_eq!(desc.version.patch, 0);
        assert!(desc.provides.is_empty());
        assert!(desc.requires.is_empty());
        assert!(desc.optional_requires.is_empty());
    }

    #[test]
    fn module_descriptor_with_provides() {
        let desc: ModuleDescriptor = module_descriptor!(
            name: "provider",
            version: "1.2.3",
            provides: ["cap:http.server", "cap:router"]
        );
        assert_eq!(desc.name, "provider");
        assert_eq!(desc.provides, &["cap:http.server", "cap:router"]);
    }

    #[test]
    fn module_descriptor_with_requires() {
        let desc: ModuleDescriptor = module_descriptor!(
            name: "consumer",
            version: "2.0.0",
            requires: ["cap:config"]
        );
        assert_eq!(desc.requires, &["cap:config"]);
    }

    #[test]
    fn module_descriptor_full() {
        let desc: ModuleDescriptor = module_descriptor!(
            name: "full_module",
            version: "3.1.4",
            provides: ["cap:service"],
            requires: ["cap:config", "cap:logging"],
            optional_requires: ["cap:metrics"],
        );
        assert_eq!(desc.name, "full_module");
        assert_eq!(desc.version.major, 3);
        assert_eq!(desc.provides, &["cap:service"]);
        assert_eq!(desc.requires, &["cap:config", "cap:logging"]);
        assert_eq!(desc.optional_requires, &["cap:metrics"]);
    }

    #[test]
    fn module_descriptor_accepts_typed_caps() {
        // Typed Cap constants via `.0` are accepted and yield the same strings as
        // the equivalent literals — this is the typo-checked form preferred in
        // airframe's own descriptors. A misspelled constant would fail to compile.
        use airframe_core::module::{CAP_CONFIG, CAP_HTTP_SERVER, CAP_METRICS};
        let desc: ModuleDescriptor = module_descriptor!(
            name: "typed_module",
            version: "1.0.0",
            provides: [CAP_HTTP_SERVER.0],
            requires: [CAP_CONFIG.0],
            optional_requires: [CAP_METRICS.0],
        );
        assert_eq!(desc.provides, &["cap:http.server"]);
        assert_eq!(desc.requires, &["cap:config"]);
        assert_eq!(desc.optional_requires, &["cap:metrics"]);
    }
}
