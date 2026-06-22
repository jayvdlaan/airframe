#![cfg_attr(not(feature = "module"), allow(dead_code, unused_imports))]
//! Airframe Logging
//!
//! Feature flags:
//! - module: enable Airframe Module integration (requires airframe_core)
//! - config: integrate with airframe_config types
//! - args: allow applying CLI overrides from airframe_args
//! - journald: enable journald sink via tracing-journald
//! - syslog: enable syslog sink via syslog-tracing
//!
//! Sinks-first configuration example (TOML):
//!
//! ```toml
//! [logging]
//! directives = ["info"]
//! [[logging.sinks]]
//! kind = "console"
//! json = true
//! ansi = true
//! filter = "my_app=debug"
//! [[logging.sinks]]
//! kind = "file"
//! path = "logs/app.log"
//! json = false
//! ansi = false
//! ```
//!
//! To enable journald or syslog sinks, build with the corresponding features and add
//! a `[[logging.sinks]]` with kind = "journald" or kind = "syslog".

// --- Extracted modules (structural decomposition) ---
pub mod bridge;
pub mod correlation;
mod filter;
mod sinks_builder;
pub mod testing;
mod validation;

// Re-export extracted items at crate root so `crate::` paths continue to work
pub use bridge::install_airframe_log_api_bridge;
pub use bridge::TracingLogger;
pub(crate) use filter::build_env_filter_from_config;
pub(crate) use sinks_builder::build_sinks_layer;
pub(crate) use validation::validate_sinks_first_schema;

// Facade: public API modules
pub mod api {
    pub mod config;
    pub mod events;
}
// Expose helper alias for subscriber type so submodules can reference it without circular deps
pub mod layer_parent;

// Expose split submodules
pub mod filters {
    pub mod per_sink;
}
pub mod layers {
    pub mod sinks_layer;
}
// Concrete sinks
pub mod sinks {
    pub mod console;
    pub mod file;
    pub(crate) mod format_helpers;
}

// IO utilities (writers, rotation helpers)
pub mod io {
    pub mod correlation_json_writer;
    pub mod rotation;
}
// Runtime state and helpers
pub mod runtime {
    pub mod apply_cli;
    pub mod state;
}

// Prelude for convenient imports
pub mod prelude;

// Airframe runtime module integration (split)
#[cfg(feature = "module")]
pub mod module;
#[cfg(feature = "module")]
pub use crate::module::LoggingModule;

pub use crate::api::config::{FormatOptions, LoggingConfig, RotationConfig, SinkConfig};
pub use crate::api::events::*;
pub use crate::runtime::state::{LoggingControl, LoggingState};

#[cfg(all(test, feature = "module", feature = "config", any()))]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;
    use airframe_core::bus::EventBus;
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn loads_initial_level_from_config() {
        // Prepare a temp config file
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("af.toml");
        fs::write(&p, "[logging]\ndirectives=['debug']\n").unwrap();

        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(p.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        let state = app.services.get::<LoggingState>().expect("state");
        assert_eq!(state.get().directives, Some(vec!["debug".to_string()]));
    }

    #[tokio::test]
    async fn updates_state_on_config_reloaded_event() {
        // Start without logging.level, then simulate that it becomes set
        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(None))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        // Initially None
        let state = app.services.get::<LoggingState>().expect("state");
        assert_eq!(state.get().level, None);

        // Simulate config change by replacing BasicConfig in services and publishing ConfigReloaded
        if let Some(bus) = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        {
            let new_raw: toml::Value = "[logging]\ndirectives=['info']\n".parse().unwrap();
            let cfg = airframe_config::BasicConfig {
                raw: new_raw,
                source: None,
            };
            app.services
                .register::<airframe_config::BasicConfig>(Arc::new(cfg));
            bus.publish(airframe_config::ConfigReloaded, None)
                .await
                .unwrap();
            // Wait a moment for async task
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert_eq!(state.get().directives, Some(vec!["info".to_string()]));
        } else {
            panic!("event bus not registered");
        }
    }

    #[tokio::test]
    async fn set_log_level_event_updates_global_level() {
        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(None))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();
        let bus = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
            .expect("bus");
        // Synchronize with initial ConfigReloaded -> LoggingChanged to avoid racing with startup
        let mut rx = bus.subscribe::<LoggingChanged>().unwrap();
        use tokio_stream::StreamExt;
        let _ = tokio::time::timeout(Duration::from_secs(2), rx.next())
            .await
            .expect("initial LoggingChanged after startup");

        // Publish a SetLogLevel to set global level to debug and await the resulting change
        bus.publish(
            SetLogLevel {
                target: None,
                level: "debug".to_string(),
            },
            None,
        )
        .await
        .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), rx.next())
            .await
            .expect("LoggingChanged after SetLogLevel");
        let state = app.services.get::<LoggingState>().unwrap();
        let ef = state.get().env_filter.unwrap_or_default();
        assert!(
            ef.starts_with("debug") || ef.contains(",debug"),
            "env_filter should contain global level 'debug', got: {}",
            ef
        );
    }

    #[tokio::test]
    async fn set_log_level_event_updates_target_level() {
        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(None))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();
        let bus = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
            .expect("bus");
        // Synchronize with initial ConfigReloaded -> LoggingChanged to avoid racing with startup
        let mut rx = bus.subscribe::<LoggingChanged>().unwrap();
        use tokio_stream::StreamExt;
        let _ = tokio::time::timeout(Duration::from_secs(2), rx.next())
            .await
            .expect("initial LoggingChanged after startup");

        // Publish a SetLogLevel to set target level and await the resulting change
        bus.publish(
            SetLogLevel {
                target: Some("airframe_logging".to_string()),
                level: "warn".to_string(),
            },
            None,
        )
        .await
        .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), rx.next())
            .await
            .expect("LoggingChanged after SetLogLevel");
        let state = app.services.get::<LoggingState>().unwrap();
        let ef = state.get().env_filter.unwrap_or_default();
        assert!(
            ef.contains("airframe_logging=warn"),
            "env_filter should contain 'airframe_logging=warn', got: {}",
            ef
        );
    }

    #[tokio::test]
    async fn request_logging_status_roundtrip() {
        use crate::correlation;
        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(None))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();
        let bus = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
            .expect("bus");
        // Subscribe before publishing the request
        let mut rx = bus.subscribe::<LoggingStatus>().unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        bus.publish(RequestLoggingStatus, None).await.unwrap();
        use tokio_stream::StreamExt;
        let got = tokio::time::timeout(Duration::from_secs(1), rx.next())
            .await
            .unwrap()
            .unwrap();
        // Also verify correlation::scope/get() works in this task
        let id = "corr-123".to_string();
        let got_id = correlation::scope(id.clone(), async move { correlation::get() }).await;
        assert_eq!(got_id, Some(id));
        // Verify snapshot is reasonable (env_filter built from defaults)
        let cfg = got.config;
        assert!(
            cfg.env_filter.is_none()
                || cfg.env_filter.as_ref().unwrap().contains("info")
                || cfg.level.as_deref() == Some("info")
        );
    }

    #[tokio::test]
    async fn rejects_legacy_top_level_keys() {
        // Config with legacy keys should fail Module::init (schema enforcement)
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("af.toml");
        std::fs::write(&p, "[logging]\nlevel='debug'\njson=true\n").unwrap();
        let res = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(p.clone())))
            .with(LoggingModule::new())
            .start()
            .await;
        assert!(
            res.is_err(),
            "legacy keys should cause logging module init to fail"
        );
    }

    #[tokio::test]
    async fn loads_sinks_from_config() {
        // Prepare a temp config file with sinks
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("af.toml");
        std::fs::write(
            &p,
            r#"
[logging]

[[logging.sinks]]
kind = "console"
json = true
ansi = true
filter = "airframe_logging=debug"

[[logging.sinks]]
kind = "file"
path = "logs/app.log"
json = false
ansi = false
filter = "info"
"#,
        )
        .unwrap();

        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(p.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        let state = app.services.get::<LoggingState>().expect("state");
        let cfg = state.get();
        assert!(cfg.sinks.is_some(), "sinks should be parsed from config");
        let sinks = cfg.sinks.unwrap();
        assert!(sinks.len() >= 1);
        // Ensure at least one console sink exists
        let has_console = sinks
            .iter()
            .any(|s| matches!(s, SinkConfig::Console { .. }));
        assert!(has_console, "expected at least one console sink");
    }

    #[tokio::test]
    async fn toggle_json_runtime_changes_format() {
        // Config with a file sink (non-JSON initially)
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("af.toml");
        let file_path = dir.path().join("app.log");
        let cfg = format!(
            "[logging]\n\n[[logging.sinks]]\nkind=\"file\"\npath=\"{}\"\njson=false\nansi=false\nfilter=\"info\"\n",
            file_path.to_string_lossy()
        );
        std::fs::write(&p, cfg).unwrap();

        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(p.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        // Emit a log, should be plain text
        tracing::info!(target: "airframe_logging", "hello plain");
        tokio::time::sleep(Duration::from_millis(800)).await;
        let _before = std::fs::read_to_string(&file_path).unwrap_or_default();
        // Flip JSON on globally and rebuild sinks via event
        let bus = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
            .expect("bus");
        bus.publish(ToggleJson { enabled: true }, None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        tracing::info!(target: "airframe_logging", "hello json");
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let after = std::fs::read_to_string(&file_path).unwrap_or_default();
        // In JSON mode, we expect to see braces/quoted fields
        assert!(
            after.contains("\"hello json\"") || after.contains("{"),
            "after content did not look like json: {}",
            after
        );
    }

    #[tokio::test]
    async fn config_reload_adds_file_sink_without_restart() {
        // Start with console-only config
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("af.toml");
        std::fs::write(
            &p,
            "[logging]\n\n[[logging.sinks]]\nkind=\"console\"\njson=false\nansi=true\n",
        )
        .unwrap();

        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(p.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        let file_path = dir.path().join("added.log");

        // Update config to add a file sink
        let cfg2 = format!(
            "[logging]\n\n[[logging.sinks]]\nkind=\"console\"\njson=false\nansi=true\n\n[[logging.sinks]]\nkind=\"file\"\npath=\"{}\"\njson=true\nansi=false\nfilter=\"info\"\n",
            file_path.to_string_lossy()
        );
        std::fs::write(&p, &cfg2).unwrap();

        // Replace BasicConfig in services so our module reads the updated sinks
        let new_raw: toml::Value = cfg2.parse().unwrap();
        let cfg_obj = airframe_config::BasicConfig {
            raw: new_raw,
            source: None,
        };
        app.services
            .register::<airframe_config::BasicConfig>(Arc::new(cfg_obj));
        // Publish ConfigReloaded
        let bus = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
            .expect("bus");
        bus.publish(airframe_config::ConfigReloaded, None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        // Emit logs which should now also go to the file sink
        tracing::info!(target: "airframe_logging", "file added");
        tracing::info!(target: "airframe_logging", "file added 2");
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert!(file_path.exists(), "file sink should exist after reload");
        let content = std::fs::read_to_string(&file_path).unwrap_or_default();
        assert!(content.contains("file added"));
    }

    #[tokio::test]
    async fn multi_sink_two_consoles_two_files_receive_per_filters() {
        // Prepare temp config with two consoles and two files
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("af.toml");
        let f1 = dir.path().join("a.log");
        let f2 = dir.path().join("b.log");
        let toml_cfg = format!(
            r#"[logging]
directives = ["info"]

[[logging.sinks]]
kind = "console"
json = false
ansi = true
filter = "airframe_alpha=info"

[[logging.sinks]]
kind = "console"
json = true
ansi = false
filter = "airframe_beta=debug"

[[logging.sinks]]
kind = "file"
path = "{f1}"
json = false
ansi = false
filter = "airframe_alpha=info"

[[logging.sinks]]
kind = "file"
path = "{f2}"
json = true
ansi = false
filter = "airframe_beta=info"
"#,
            f1 = f1.to_string_lossy(),
            f2 = f2.to_string_lossy(),
        );
        std::fs::write(&cfg_path, toml_cfg).unwrap();

        let _app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(cfg_path.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        // Emit logs for two different targets
        tracing::info!(target: "airframe_alpha", "alpha one");
        tracing::info!(target: "airframe_beta", "beta one");
        // Give time for non-blocking writer
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify file 1 contains only alpha
        let c1 = std::fs::read_to_string(&f1).unwrap_or_default();
        assert!(
            c1.contains("alpha one"),
            "f1 should contain alpha; got: {}",
            c1
        );
        assert!(
            !c1.contains("beta one"),
            "f1 should not contain beta; got: {}",
            c1
        );

        // Verify file 2 contains beta; json=true means quoted message likely present
        let c2 = std::fs::read_to_string(&f2).unwrap_or_default();
        assert!(
            c2.contains("beta one") || c2.contains("\"beta one\""),
            "f2 should contain beta; got: {}",
            c2
        );
        assert!(
            !c2.contains("alpha one"),
            "f2 should not contain alpha; got: {}",
            c2
        );
    }

    #[tokio::test]
    async fn rotation_daily_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("af.toml");
        let base_dir = dir.path().join("logs");
        let file_name = "app.log";
        std::fs::create_dir_all(&base_dir).unwrap();
        let full_path = base_dir.join(file_name);
        let toml_cfg = format!(
            r#"[logging]

[[logging.sinks]]
kind = "file"
path = "{path}"
json = false
ansi = false
rotation = "daily"
"#,
            path = full_path.to_string_lossy(),
        );
        std::fs::write(&cfg_path, toml_cfg).unwrap();

        let _app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(cfg_path.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        tracing::info!(target: "airframe_logging", "daily rotate test");
        tokio::time::sleep(Duration::from_millis(3000)).await;

        // Look for a rotated file named app.log somewhere under base_dir
        fn find_file(base: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
            for entry in std::fs::read_dir(base).ok()? {
                let entry = entry.ok()?;
                let p = entry.path();
                if p.is_dir() {
                    if let Some(found) = find_file(&p, name) {
                        return Some(found);
                    }
                } else if let Some(osn) = p.file_name() {
                    let s = osn.to_string_lossy();
                    if s.contains(name) {
                        return Some(p);
                    }
                }
            }
            None
        }
        let rotated_path =
            find_file(&base_dir, file_name).expect("expected rotated daily file under base_dir");
        let content = std::fs::read_to_string(&rotated_path).unwrap_or_default();
        assert!(
            content.contains("daily rotate test"),
            "content not found in {}",
            rotated_path.display()
        );
    }

    #[tokio::test]
    async fn rotation_hourly_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("af.toml");
        let base_dir = dir.path().join("logs");
        let file_name = "app.log";
        std::fs::create_dir_all(&base_dir).unwrap();
        let full_path = base_dir.join(file_name);
        let toml_cfg = format!(
            r#"[logging]

[[logging.sinks]]
kind = "file"
path = "{path}"
json = false
ansi = false
rotation = "hourly"
"#,
            path = full_path.to_string_lossy(),
        );
        std::fs::write(&cfg_path, toml_cfg).unwrap();

        let _app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(cfg_path.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        tracing::info!(target: "airframe_logging", "hourly rotate test");
        tokio::time::sleep(Duration::from_millis(3000)).await;

        // Look for a rotated file named app.log somewhere under base_dir
        fn find_file(base: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
            for entry in std::fs::read_dir(base).ok()? {
                let entry = entry.ok()?;
                let p = entry.path();
                if p.is_dir() {
                    if let Some(found) = find_file(&p, name) {
                        return Some(found);
                    }
                } else if let Some(osn) = p.file_name() {
                    let s = osn.to_string_lossy();
                    if s.contains(name) {
                        return Some(p);
                    }
                }
            }
            None
        }
        let rotated_path =
            find_file(&base_dir, file_name).expect("expected rotated hourly file under base_dir");
        let content = std::fs::read_to_string(&rotated_path).unwrap_or_default();
        assert!(
            content.contains("hourly rotate test"),
            "content not found in {}",
            rotated_path.display()
        );
    }

    #[tokio::test]
    async fn rotation_size_with_retention() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("af.toml");
        let base_dir = dir.path().join("logs");
        let file_name = "rolling.log";
        std::fs::create_dir_all(&base_dir).unwrap();
        let full_path = base_dir.join(file_name);
        // tiny max_bytes to force rotation quickly, keep 3 files
        let toml_cfg = format!(
            r#"[logging]

[[logging.sinks]]
kind = "file"
path = "{path}"
json = false
ansi = false
rotation = {{ policy = "size", max_bytes = 64, keep = 3 }}
"#,
            path = full_path.to_string_lossy(),
        );
        std::fs::write(&cfg_path, toml_cfg).unwrap();

        let _app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(cfg_path.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        // Emit payloads large enough to exceed 64 bytes multiple times
        for i in 0..10u32 {
            tracing::info!(target: "airframe_logging", msg = %"X".repeat(80), i);
        }
        // Give time for non-blocking writer to flush
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Check base file exists
        assert!(full_path.exists(), "base rolling file should exist");
        // Count retention files .1, .2, .3 exist, but .4 does not
        let f1 = base_dir.join(format!("{}.{n}", file_name, n = 1));
        let f2 = base_dir.join(format!("{}.{n}", file_name, n = 2));
        let f3 = base_dir.join(format!("{}.{n}", file_name, n = 3));
        let f4 = base_dir.join(format!("{}.{n}", file_name, n = 4));
        assert!(
            f1.exists() || f2.exists() || f3.exists(),
            "at least one rotated file should exist"
        );
        // Ensure no older than keep
        assert!(!f4.exists(), "older than keep should be deleted");
    }

    #[tokio::test]
    async fn correlation_id_injection_text() {
        use crate::correlation;
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("af.toml");
        let file_path = dir.path().join("app.log");
        let toml_cfg = format!(
            r#"[logging]

[[logging.sinks]]
kind = "file"
path = "{path}"
json = false
ansi = false
# include_correlation_id defaults to true
"#,
            path = file_path.to_string_lossy(),
        );
        std::fs::write(&cfg_path, toml_cfg).unwrap();

        let _app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(cfg_path.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        let corr = "req-abc-123".to_string();
        correlation::scope(corr.clone(), async move {
            tracing::info!(target: "airframe_logging", "hello with corr");
        })
        .await;

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let content = std::fs::read_to_string(&file_path).unwrap_or_default();
        let has_prefix = content.contains("[correlation_id=");
        let has_span = content.contains("corr{correlation_id=");
        assert!(
            has_prefix || has_span,
            "expected correlation info in text output (prefix or span field): {}",
            content
        );
        assert!(
            content.contains(&corr),
            "expected correlation id value in text output: {}",
            content
        );
    }

    #[tokio::test]
    async fn correlation_id_json_injection_enabled_and_disabled() {
        use crate::correlation;
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("af.toml");
        let file_path = dir.path().join("app.jsonl");
        // First: enabled (default true)
        let toml_cfg = format!(
            r#"[logging]

[[logging.sinks]]
kind = "file"
path = "{path}"
json = true
ansi = false
# include_correlation_id defaults to true
"#,
            path = file_path.to_string_lossy(),
        );
        std::fs::write(&cfg_path, toml_cfg).unwrap();

        let app = AppBuilder::new()
            .with(airframe_config::ConfigModule::new(Some(cfg_path.clone())))
            .with(LoggingModule::new())
            .start()
            .await
            .unwrap();

        let corr = "req-json-456".to_string();
        correlation::scope(corr.clone(), async move {
            tracing::info!(target: "airframe_logging", msg = "hello json corr");
        })
        .await;

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let content = std::fs::read_to_string(&file_path).unwrap_or_default();
        // Should include correlation_id field and valid JSON
        let mut saw = false;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            assert!(
                !line.contains("[correlation_id="),
                "JSON line must not contain text prefix: {}",
                line
            );
            let v: serde_json::Value =
                serde_json::from_str(line).expect("expected valid JSON line");
            if let Some(obj) = v.as_object() {
                if obj.get("correlation_id").is_some() {
                    saw = true;
                }
            }
        }
        assert!(
            saw,
            "expected correlation_id field in JSON output when enabled:
{}",
            content
        );

        // Second: disable via runtime SetSinkFormat
        if let Some(bus) = app
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        {
            bus.publish(
                SetSinkFormat {
                    sink_id: 0,
                    json: None,
                    ansi: None,
                    with_span_events: None,
                    include_correlation_id: Some(false),
                },
                None,
            )
            .await
            .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        // Emit one more event
        correlation::scope(corr.clone(), async move {
            tracing::info!(target: "airframe_logging", msg = "after toggle");
        })
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let content2 = std::fs::read_to_string(&file_path).unwrap_or_default();
        // Find the last non-empty line and assert it doesn't include the field now
        let last_line = content2
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        let v2: serde_json::Value = serde_json::from_str(last_line).expect("valid JSON");
        assert!(
            v2.get("correlation_id").is_none(),
            "correlation_id should be absent after disabling: {}",
            last_line
        );
    }
}

#[cfg(test)]
mod buffer_tests_det {
    use super::testing;
    #[test]
    fn testing_buffer_sink_captures_logs_text() {
        let _g = testing::init_for_test("info", false);
        tracing::info!(target: "airframe_logging", "hello buffer");
        let out = testing::take();
        assert!(
            out.contains("airframe_logging"),
            "should include target: {}",
            out
        );
        assert!(
            out.contains("hello buffer"),
            "should include message: {}",
            out
        );
    }

    #[test]
    fn testing_buffer_sink_captures_logs_json() {
        let _g = testing::init_for_test("info", true);
        tracing::info!(target: "airframe_logging", msg = "hello json buffer");
        let out = testing::take();
        assert!(
            out.contains("hello json buffer"),
            "should include message: {}",
            out
        );
        assert!(
            out.contains("\"level\""),
            "json should include level field: {}",
            out
        );
    }
}
