//! airframe_config — functional config utilities with optional Airframe Module integration.
//!
//! Layout:
//! - api/ — types like BasicConfig, ConfigReloaded
//! - io/ — helpers for files, env, cli
//! - resolve.rs — file selection/precedence
//! - reload.rs — hot-reload watcher (feature = "module")
//! - module.rs — ConfigModule runtime integration (feature = "module")
//!
//! Features:
//! - module: enable Airframe runtime integration (brings optional deps)
//! - args: enable CLI helpers/ArgsModule interop where applicable

// New skeleton module layout (facade re-exports)
pub mod api;
pub mod config_listener;
pub mod defaults_registry;
pub mod io;
pub mod module;
pub mod reload;
pub mod resolve;

pub mod prelude;

#[cfg(feature = "module")]
pub use crate::config_listener::get_or_create_config_listener_registry;
pub use crate::config_listener::{ConfigListener, ConfigListenerRegistry};
#[cfg(feature = "module")]
pub use crate::defaults_registry::get_or_create_config_defaults_registry;
pub use crate::defaults_registry::{ConfigDefaultsContributor, ConfigDefaultsRegistry};
#[cfg(feature = "module")]
pub use crate::module::ConfigModule;

// Move core types into api::types and re-export here
pub use crate::api::types::{BasicConfig, ConfigReloaded, ConfigWatcherReady};

#[cfg(all(test, feature = "module"))]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_core::bus::EventBus;
    use airframe_core::module::Module;
    use anyhow::anyhow;
    use async_trait::async_trait;
    use semver::Version;
    use serial_test::serial;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio_stream::StreamExt;

    // Helper to safely set/unset environment variables within a test and restore afterward
    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    #[tokio::test]
    #[serial]
    async fn hot_reload_updates_and_publishes_event() {
        use airframe_core::bus::EventBus;
        use std::{fs, time::Duration};
        use tokio_stream::StreamExt;

        // Single source file
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("reload.toml");
        fs::write(&p, "k=1\n").unwrap();

        let app = AppBuilder::new()
            .with(ConfigModule::new(Some(p.clone())))
            .start()
            .await
            .unwrap();

        // Subscribe to ConfigReloaded
        let bus = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
            .unwrap();
        let mut rx = bus.subscribe::<ConfigReloaded>().unwrap();

        // Consume the initial ConfigReloaded published during init
        let _ = tokio::time::timeout(Duration::from_secs(2), rx.next())
            .await
            .expect("initial event");

        // Modify the file; avoid fixed sleeps and await the next event deterministically
        fs::write(&p, "k=2\n").unwrap();

        // Expect a reload event (second event) with a generous timeout to avoid CI flakiness
        let _ = tokio::time::timeout(Duration::from_secs(10), rx.next())
            .await
            .expect("expected a reload event after file change");

        // Config should reflect the new value; in rare cases, the event may be published just
        // before the BasicConfig swap is visible to this task. Poll briefly until observed.
        let bc = app.services.get::<BasicConfig>().unwrap();
        let mut waited_ms = 0u64;
        while bc.get::<i64>("k") != 2 && waited_ms < 5000 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            waited_ms += 20;
        }
        assert_eq!(bc.get::<i64>("k"), 2);
    }

    #[tokio::test]
    #[serial]
    async fn hot_reload_disabled_does_not_watch() {
        use airframe_core::bus::EventBus;
        use std::{fs, time::Duration};
        use tokio_stream::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nowatch.toml");
        fs::write(&p, "k=1\n").unwrap();

        let app = AppBuilder::new()
            .with(ConfigModule::new(Some(p.clone())).with_hot_reload(false))
            .start()
            .await
            .unwrap();

        // Subscribe to ConfigReloaded for events after initialization
        let bus = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
            .unwrap();
        let mut rx = bus.subscribe::<ConfigReloaded>().unwrap();

        // Drain the initial event produced during module init
        let _ = tokio::time::timeout(Duration::from_secs(2), rx.next())
            .await
            .expect("initial event");

        // Change the file; with hot-reload disabled, nothing should happen
        fs::write(&p, "k=2\n").unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Assert no event is published within the timeout window
        let no_event = tokio::time::timeout(Duration::from_millis(700), rx.next()).await;
        assert!(
            no_event.is_err(),
            "no reload event expected when hot-reload is disabled"
        );

        // And the BasicConfig value remains unchanged
        let bc = app.services.get::<BasicConfig>().unwrap();
        assert_eq!(bc.get::<i64>("k"), 1);
    }
    impl EnvGuard {
        // Remove the variable for the guard's lifetime
        fn removed(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, prev }
        }
        // Set the variable to a value for the guard's lifetime
        fn set(key: &'static str, val: String) -> Self {
            let prev = std::env::var(key).ok();
            std::env::set_var(key, &val);
            Self { key, prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(v) = &self.prev {
                std::env::set_var(self.key, v);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[tokio::test]
    #[serial]
    async fn registers_default_config_when_missing() {
        // ensure no external config path influences this test
        let _g = EnvGuard::removed("AIRFRAME_CONFIG_PATH");
        let module = ConfigModule::new(None);
        let builder = {
            #[cfg(feature = "args")]
            {
                AppBuilder::new().with(make_test_args(vec![]))
            }
            #[cfg(not(feature = "args"))]
            {
                AppBuilder::new()
            }
        };
        let app = builder.with(module).start().await.unwrap();
        let cfg = app.services.get::<BasicConfig>().expect("config present");
        // Do not assert on source; environment variables set by other parallel tests can influence file selection.
        assert!(cfg.raw.is_table());
    }

    #[tokio::test]
    async fn publishes_config_reloaded_event() {
        struct SubMod(airframe_core::module::ModuleDescriptor);
        #[async_trait]
        impl Module for SubMod {
            fn descriptor(&self) -> &airframe_core::module::ModuleDescriptor {
                &self.0
            }
            async fn init(
                &mut self,
                ctx: airframe_core::module::ModuleContext,
            ) -> anyhow::Result<()> {
                let bus = ctx
                    .services
                    .get::<airframe_core::bus::inmem::InMemoryEventBus>()
                    .expect("event bus");
                let mut stream = bus.subscribe::<ConfigReloaded>()?;
                let cancel = ctx.cancel.clone();
                tokio::spawn(async move {
                    let _ = stream.next().await;
                    cancel.cancel();
                });
                Ok(())
            }
        }

        let sub = SubMod(airframe_core::module::ModuleDescriptor {
            name: "sub",
            version: semver::Version::parse("0.1.0").unwrap(),
            provides: &[],
            requires: &[],
            optional_requires: &[],
            requires_with_versions: &[],
            optional_requires_with_versions: &[],
        });
        let cfg = ConfigModule::new(None);
        let builder = {
            #[cfg(feature = "args")]
            {
                AppBuilder::new().with(make_test_args(vec![]))
            }
            #[cfg(not(feature = "args"))]
            {
                AppBuilder::new()
            }
        };
        let app = builder.with(sub).with(cfg).start().await.unwrap();
        // Wait until the subscriber cancels after receiving the event
        app.run_until_cancelled().await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn layering_precedence_defaults_files_env_cli() {
        // defaults
        let defaults: toml::Value = "[logging]\nlevel='info'\n".parse().unwrap();
        // file: warn
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.toml");
        std::fs::write(&path, "[logging]\nlevel='warn'\n").unwrap();
        // env: error
        let _g_log = EnvGuard::set("AIRFRAME__logging__level", "error".to_string());
        // cli: debug
        let cli = vec!["--cfg.logging.level=debug".to_string()];

        let module = ConfigModule::new(Some(path.clone()))
            .with_defaults(defaults)
            .with_cli_overrides(cli);
        let builder = {
            #[cfg(feature = "args")]
            {
                AppBuilder::new().with(make_test_args(vec![]))
            }
            #[cfg(not(feature = "args"))]
            {
                AppBuilder::new()
            }
        };
        let app = builder.with(module).start().await.unwrap();
        let cfg = app.services.get::<BasicConfig>().unwrap();
        let level = cfg
            .raw
            .get("logging")
            .and_then(|t| t.get("level"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(level, "debug"); // CLI wins over env/file/defaults

        // EnvGuard cleans up
    }

    #[tokio::test]
    async fn validation_error_is_returned() {
        let defaults: toml::Value = "[app]\ninvalid=true\n".parse().unwrap();
        let module = ConfigModule::new(None)
            .with_defaults(defaults)
            .with_validator(|raw| {
                if raw
                    .get("app")
                    .and_then(|t| t.get("invalid"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    Err(anyhow!("invalid config"))
                } else {
                    Ok(())
                }
            });
        let builder = {
            #[cfg(feature = "args")]
            {
                AppBuilder::new().with(make_test_args(vec![]))
            }
            #[cfg(not(feature = "args"))]
            {
                AppBuilder::new()
            }
        };
        let err = builder
            .with(module)
            .start()
            .await
            .err()
            .expect("should fail");
        let msg = format!("{}", err);
        assert!(msg.contains("invalid config"));
    }

    // 5.2 `with_validator` blocks invalid configs
    #[tokio::test]
    async fn validator_blocks_invalid_config() {
        use airframe_core::app::AppBuilder;
        // No defaults or files provide the required key
        let module = ConfigModule::new(None).with_validator(|raw| {
            // Require a dotted path key: required.key
            if raw.get("required").and_then(|t| t.get("key")).is_none() {
                Err(anyhow!("missing required key: required.key"))
            } else {
                Ok(())
            }
        });
        // Build App without adding ArgsModule explicitly; this test must pass regardless of args feature
        let err = AppBuilder::new()
            .with(module)
            .start()
            .await
            .err()
            .expect("should fail");
        let msg = format!("{}", err);
        assert!(msg.contains("missing required key: required.key"));
    }

    // 5.3 `with_cli_overrides` works without `ArgsModule`
    #[tokio::test]
    #[serial]
    async fn cli_overrides_without_args_feature_or_module() {
        use airframe_core::app::AppBuilder;
        // Provide defaults and override via with_cli_overrides
        let defaults: toml::Value = "[logging]\nlevel='info'\n".parse().unwrap();
        let module = ConfigModule::new(None)
            .with_defaults(defaults)
            .with_cli_overrides(vec!["--cfg.logging.level=debug".to_string()]);

        // Intentionally do not provide any ArgsModule even if feature = "args" is enabled
        let app = AppBuilder::new().with(module).start().await.unwrap();

        let cfg = app.services.get::<BasicConfig>().unwrap();
        let level = cfg
            .raw
            .get("logging")
            .and_then(|t| t.get("level"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            level, "debug",
            "with_cli_overrides must apply even without ArgsModule"
        );
    }

    // 8) Feature matrix: ensure module initializes without `args` feature.
    // Env merging is opt-in via `with_env_prefixes`.
    #[tokio::test]
    #[serial]
    async fn config_module_initializes_without_args() {
        use airframe_core::app::AppBuilder;
        // Ensure no config path via env interferes
        let _guard = EnvGuard::removed("AIRFRAME_CONFIG_PATH");
        // Ensure any previous tests' logging level env override does not leak into this test
        let _g_ll_uc = EnvGuard::removed("AIRFRAME__LOGGING__LEVEL");
        let _g_ll_lc = EnvGuard::removed("AIRFRAME__logging__level");

        // Set an env override that should be merged when env prefixes are enabled
        let _g_app = EnvGuard::set("AIRFRAME__APP__NAME", "from_env".to_string());

        // Provide a simple default so we can assert both defaults and env merge
        let defaults: toml::Value = "[logging]\nlevel='info'\n".parse().unwrap();

        // Start App with only ConfigModule (do NOT add ArgsModule regardless of feature flags)
        let app = AppBuilder::new()
            .with(
                ConfigModule::new(None)
                    .with_defaults(defaults)
                    .with_env_prefixes(vec!["AIRFRAME__"]),
            )
            .start()
            .await
            .expect("config module should initialize without args");

        let cfg = app.services.get::<BasicConfig>().unwrap();
        // Defaults should land
        let level = cfg
            .raw
            .get("logging")
            .and_then(|t| t.get("level"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(level, "info");

        // Env override should land
        let app_name = cfg
            .raw
            .get("app")
            .and_then(|t| t.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(app_name, "from_env");
    }

    // 6.3 `source` semantics: None for 0 files, Some(path) for exactly 1 file, None for 2+ files
    #[tokio::test]
    #[serial]
    async fn source_is_some_only_for_single_file_zero() {
        use airframe_core::app::AppBuilder;
        // ensure no env influences
        let _g = EnvGuard::removed("AIRFRAME_CONFIG_PATH");
        let app = AppBuilder::new()
            .with(ConfigModule::new(None))
            .start()
            .await
            .unwrap();
        let bc = app.services.get::<BasicConfig>().unwrap();
        assert!(bc.source.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn source_is_some_only_for_single_file_one() {
        use airframe_core::app::AppBuilder;
        let _g = EnvGuard::removed("AIRFRAME_CONFIG_PATH");
        let dir = tempfile::tempfile().unwrap();
        // tempfile() gives file handle; persist path by writing then getting path via into_temp_path? Simpler: use tempdir
        drop(dir);
        let td = tempfile::tempdir().unwrap();
        let p = td.path().join("one.toml");
        std::fs::write(&p, "k=1\n").unwrap();
        let app = AppBuilder::new()
            .with(ConfigModule::new(Some(p.clone())))
            .start()
            .await
            .unwrap();
        let bc = app.services.get::<BasicConfig>().unwrap();
        assert_eq!(bc.source.as_deref(), Some(p.as_path()));
    }

    #[tokio::test]
    #[serial]
    async fn source_is_some_only_for_single_file_two() {
        use airframe_core::app::AppBuilder;
        // Use only env var with two files
        let td = tempfile::tempdir().unwrap();
        let p1 = td.path().join("a.toml");
        let p2 = td.path().join("b.toml");
        std::fs::write(&p1, "a=1\n").unwrap();
        std::fs::write(&p2, "b=2\n").unwrap();
        // Set env to two paths
        #[cfg(windows)]
        let sep = ";";
        #[cfg(not(windows))]
        let sep = ":";
        let _g = EnvGuard::set(
            "AIRFRAME_CONFIG_PATH",
            format!("{}{}{}", p1.display(), sep, p2.display()),
        );
        let app = AppBuilder::new()
            .with(ConfigModule::new(None))
            .start()
            .await
            .unwrap();
        let bc = app.services.get::<BasicConfig>().unwrap();
        assert!(bc.source.is_none());
        // EnvGuard cleans up
    }

    #[tokio::test]
    #[serial]
    async fn hot_reload_debounce_single_event_per_burst() {
        // ensure clean env so only the default single file is used as source
        std::env::remove_var("AIRFRAME_CONFIG_PATH");
        // prepare file and module
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.toml");
        std::fs::write(&path, "[x]\na=1\n").unwrap();

        // Ready signals:
        //  - initial_rx waits for the initial ConfigReloaded
        //  - watcher_rx waits for ConfigWatcherReady (file watcher installed)
        let (initial_tx, initial_rx) = tokio::sync::oneshot::channel::<()>();
        let (watcher_tx, watcher_rx) = tokio::sync::oneshot::channel::<()>();

        // subscriber module that counts events after the initial one
        struct Counter(
            airframe_core::module::ModuleDescriptor,
            Arc<tokio::sync::Mutex<usize>>,
            Option<tokio::sync::oneshot::Sender<()>>, // initial config reloaded seen
            Option<tokio::sync::oneshot::Sender<()>>, // watcher ready seen
        );
        #[async_trait]
        impl Module for Counter {
            fn descriptor(&self) -> &airframe_core::module::ModuleDescriptor {
                &self.0
            }
            async fn init(
                &mut self,
                ctx: airframe_core::module::ModuleContext,
            ) -> anyhow::Result<()> {
                let bus = ctx
                    .services
                    .get::<airframe_core::bus::inmem::InMemoryEventBus>()
                    .expect("event bus");
                let mut stream = bus.subscribe::<ConfigReloaded>()?;
                let mut ready_stream = bus.subscribe::<ConfigWatcherReady>()?;
                let counter = self.1.clone();
                let mut initial_ready = self.2.take();
                let mut watcher_ready = self.3.take();
                tokio::spawn(async move {
                    // Wait for and acknowledge the initial event before counting further ones
                    if stream.next().await.is_some() {
                        if let Some(tx) = initial_ready.take() {
                            let _ = tx.send(());
                        }
                        while tokio::time::timeout(Duration::from_secs(2), stream.next())
                            .await
                            .ok()
                            .flatten()
                            .is_some()
                        {
                            let mut c = counter.lock().await;
                            *c += 1;
                        }
                    }
                });
                // Separate task to signal when watcher is ready
                tokio::spawn(async move {
                    if ready_stream.next().await.is_some() {
                        if let Some(tx) = watcher_ready.take() {
                            let _ = tx.send(());
                        }
                    }
                });
                Ok(())
            }
        }

        let counter = Arc::new(tokio::sync::Mutex::new(0usize));
        let sub = Counter(
            airframe_core::module::ModuleDescriptor {
                name: "counter",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[],
                requires: &[],
                optional_requires: &[],
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
            counter.clone(),
            Some(initial_tx),
            Some(watcher_tx),
        );
        let cfg = ConfigModule::new(Some(path.clone()));
        let builder = {
            #[cfg(feature = "args")]
            {
                AppBuilder::new().with(make_test_args(vec![]))
            }
            #[cfg(not(feature = "args"))]
            {
                AppBuilder::new()
            }
        };
        let _app = builder.with(sub).with(cfg).start().await.unwrap();

        // Wait until the initial ConfigReloaded has definitely been observed
        let _ = initial_rx.await;
        // Also wait for the watcher to be fully installed
        let _ = watcher_rx.await;

        // Burst of changes
        for i in 0..3 {
            std::fs::write(&path, format!("[x]\na={}\n", i + 2)).unwrap();
        }

        // Allow debounce window + processing
        tokio::time::sleep(Duration::from_millis(600)).await;

        let n = { *counter.lock().await };
        assert_eq!(n, 1, "expected one reloaded event for the burst");
    }

    #[tokio::test]
    #[serial]
    async fn hot_reload_preserves_cli_and_env_overrides() {
        // ensure clean env for config path
        let _g_cfg = EnvGuard::removed("AIRFRAME_CONFIG_PATH");
        // defaults: info
        let defaults: toml::Value = "[logging]\nlevel='info'\n".parse().unwrap();
        // file: warn
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.toml");
        std::fs::write(&path, "[logging]\nlevel='warn'\n").unwrap();
        // env: error
        std::env::set_var("AIRFRAME__logging__level", "error");
        // CLI: debug
        let cli = vec!["--cfg.logging.level=debug".to_string()];

        let module = ConfigModule::new(Some(path.clone()))
            .with_defaults(defaults)
            .with_cli_overrides(cli);
        let builder = {
            #[cfg(feature = "args")]
            {
                AppBuilder::new().with(make_test_args(vec![]))
            }
            #[cfg(not(feature = "args"))]
            {
                AppBuilder::new()
            }
        };
        let app = builder.with(module).start().await.unwrap();

        // Give watcher a moment to initialize before making changes to reduce flakiness in CI
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Update file to trace and wait for debounce+reload
        std::fs::write(&path, "[logging]\nlevel='trace'\n").unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;

        let cfg = app.services.get::<BasicConfig>().unwrap();
        let level = cfg
            .raw
            .get("logging")
            .and_then(|t| t.get("level"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(level, "debug", "CLI override should persist across reloads");

        // EnvGuard cleans up
    }

    #[cfg(feature = "args")]
    struct TestArgsModule {
        argv: Vec<String>,
        desc: airframe_core::module::ModuleDescriptor,
    }

    #[cfg(feature = "args")]
    #[async_trait]
    impl Module for TestArgsModule {
        fn descriptor(&self) -> &airframe_core::module::ModuleDescriptor {
            &self.desc
        }
        async fn init(&mut self, ctx: airframe_core::module::ModuleContext) -> anyhow::Result<()> {
            let mut raw = Vec::with_capacity(self.argv.len() + 1);
            raw.push("testbin".to_string());
            raw.extend(self.argv.clone());
            ctx.services.register::<airframe_args::CliArgs>(Arc::new(
                airframe_args::CliArgs::new_normalized(raw),
            ));
            Ok(())
        }
    }

    #[cfg(feature = "args")]
    fn make_test_args(argv: Vec<&str>) -> TestArgsModule {
        TestArgsModule {
            argv: argv.into_iter().map(|s| s.to_string()).collect(),
            desc: airframe_core::module::ModuleDescriptor {
                name: "test_args",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[airframe_core::module::CAP_ARGS.0],
                requires: &[],
                optional_requires: &[],
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
        }
    }

    #[tokio::test]
    #[cfg(feature = "args")]
    async fn cli_config_path_precedence_over_env_and_default() {
        let dir = tempfile::tempdir().unwrap();
        let f_env = dir.path().join("env.toml");
        let f_cli = dir.path().join("cli.toml");
        let f_def = dir.path().join("def.toml");
        std::fs::write(&f_env, "[x]\na=1\n").unwrap();
        std::fs::write(&f_cli, "[x]\na=2\n").unwrap();
        std::fs::write(&f_def, "[x]\na=0\n").unwrap();

        // env points to f_env
        std::env::set_var("AIRFRAME_CONFIG_PATH", f_env.to_str().unwrap());

        let args = make_test_args(vec!["--config", f_cli.to_str().unwrap()]);
        let app = AppBuilder::new()
            .with(args)
            .with(ConfigModule::new(Some(f_def.clone())))
            .start()
            .await
            .unwrap();

        // CLI should win
        let cfg = app.services.get::<BasicConfig>().unwrap();
        assert_eq!(cfg.source.as_deref(), Some(f_cli.as_path()));
        let a = cfg
            .raw
            .get("x")
            .and_then(|t| t.get("a"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0);
        assert_eq!(a, 2);

        std::env::remove_var("AIRFRAME_CONFIG_PATH");
    }

    #[tokio::test]
    #[cfg(feature = "args")]
    async fn cli_multiple_paths_disable_source_and_merge_in_order() {
        std::env::remove_var("AIRFRAME_CONFIG_PATH");
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a.toml");
        let f2 = dir.path().join("b.toml");
        std::fs::write(&f1, "[x]\na=1\n").unwrap();
        std::fs::write(&f2, "[x]\na=2\n").unwrap();

        // Pass both paths via CLI; last wins, and source = None
        let spec = format!("{}:{}", f1.to_str().unwrap(), f2.to_str().unwrap());
        let args = make_test_args(vec!["--config", &spec]);
        let app = AppBuilder::new()
            .with(args)
            .with(ConfigModule::new(None))
            .start()
            .await
            .unwrap();

        let cfg = app.services.get::<BasicConfig>().unwrap();
        assert!(cfg.source.is_none());
        let a = cfg
            .raw
            .get("x")
            .and_then(|t| t.get("a"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0);
        assert_eq!(a, 2);
    }

    #[tokio::test]
    #[cfg(feature = "args")]
    async fn cli_strict_mode_errors_on_missing_file() {
        std::env::remove_var("AIRFRAME_CONFIG_PATH");
        let missing = std::path::PathBuf::from("./definitely_missing_config_file_12345.toml");
        let spec = missing.to_string_lossy().to_string();
        let args = make_test_args(vec!["--config", &spec]);
        let builder = AppBuilder::new()
            .with(args)
            .with(ConfigModule::new(None).with_strict_file_selection(true));
        let err = builder
            .start()
            .await
            .err()
            .expect("should error in strict mode");
        let msg = format!("{}", err);
        assert!(msg.contains("does not exist"));
    }

    #[test]
    #[cfg(windows)]
    fn windows_split_paths_respects_drive_colons() {
        // C: and D: should not cause splits on ':'
        let s = r"C:\\foo\\a.toml;D:\\bar\\b.toml";
        let v = crate::io::files::split_paths(s);
        assert_eq!(v.len(), 2);
        assert!(v[0].to_string_lossy().starts_with("C:"));
        assert!(v[1].to_string_lossy().starts_with("D:"));
    }

    #[tokio::test]
    async fn hot_reload_can_be_disabled_via_builder() {
        // prepare file and module
        std::env::remove_var("AIRFRAME_CONFIG_PATH");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.toml");
        std::fs::write(&path, "[x]\na=1\n").unwrap();

        // subscriber counting reloads after the initial event
        struct Counter(
            airframe_core::module::ModuleDescriptor,
            Arc<tokio::sync::Mutex<usize>>,
        );
        #[async_trait]
        impl Module for Counter {
            fn descriptor(&self) -> &airframe_core::module::ModuleDescriptor {
                &self.0
            }
            async fn init(
                &mut self,
                ctx: airframe_core::module::ModuleContext,
            ) -> anyhow::Result<()> {
                let bus = ctx
                    .services
                    .get::<airframe_core::bus::inmem::InMemoryEventBus>()
                    .expect("event bus");
                let mut stream = bus.subscribe::<ConfigReloaded>()?;
                let counter = self.1.clone();
                tokio::spawn(async move {
                    // Skip the initial event, then count subsequent events
                    if stream.next().await.is_some() {
                        while tokio::time::timeout(Duration::from_millis(500), stream.next())
                            .await
                            .ok()
                            .flatten()
                            .is_some()
                        {
                            let mut c = counter.lock().await;
                            *c += 1;
                        }
                    }
                });
                Ok(())
            }
        }

        let counter = Arc::new(tokio::sync::Mutex::new(0usize));
        let sub = Counter(
            airframe_core::module::ModuleDescriptor {
                name: "counter2",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[],
                requires: &[],
                optional_requires: &[],
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
            counter.clone(),
        );
        let cfg = ConfigModule::new(Some(path.clone())).with_hot_reload(false);
        let builder = {
            #[cfg(feature = "args")]
            {
                AppBuilder::new().with(make_test_args(vec![]))
            }
            #[cfg(not(feature = "args"))]
            {
                AppBuilder::new()
            }
        };
        let _app = builder.with(sub).with(cfg).start().await.unwrap();

        // Change file; since hot-reload disabled, no additional events should be observed
        std::fs::write(&path, "[x]\na=2\n").unwrap();
        tokio::time::sleep(Duration::from_millis(700)).await;
        let n = { *counter.lock().await };
        assert_eq!(
            n, 0,
            "no reload events expected when hot-reload is disabled"
        );
    }

    // ---
    // 9) Regression tests tailored to the incident
    // 9.1 CLI `--cfg.*` actually lands in `BasicConfig`
    #[tokio::test]
    #[cfg(feature = "args")]
    async fn cli_cfg_overrides_reach_basicconfig() {
        // Ensure env path doesn't interfere
        let _g = EnvGuard::removed("AIRFRAME_CONFIG_PATH");

        // Provide a CLI override via ArgsModule: --cfg.secrets.cache_key.source=cli_probe
        let args = make_test_args(vec!["--cfg.secrets.cache_key.source=cli_probe"]);
        let app = AppBuilder::new()
            .with(args)
            .with(ConfigModule::new(None))
            .start()
            .await
            .unwrap();

        let bc = app.services.get::<BasicConfig>().unwrap();
        let val: String = bc.get("secrets.cache_key.source");
        assert_eq!(val, "cli_probe");
    }

    // 9.2 `AIRFRAME_CONFIG_PATH` honored, even without `ArgsModule`
    #[tokio::test]
    #[serial]
    async fn env_path_honored_without_args_module() {
        // Create a config file with an easily visible key
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("env_only.toml");
        std::fs::write(&p, "[visible]\nkey='from_env_file'\n").unwrap();

        // Set AIRFRAME_CONFIG_PATH to point at the file; do NOT register ArgsModule
        let _g = EnvGuard::set("AIRFRAME_CONFIG_PATH", p.to_string_lossy().to_string());

        let builder = AppBuilder::new();
        let app = builder
            .with(ConfigModule::new(None).with_hot_reload(false))
            .start()
            .await
            .unwrap();

        let bc = app.services.get::<BasicConfig>().unwrap();
        let v: String = bc.get("visible.key");
        assert_eq!(v, "from_env_file");
    }
}
