//! Hot-reload and watcher/debounce helpers.
//!
//! This module hosts file watching and debounce logic used by ConfigModule.

#![cfg(feature = "module")]

use std::path::PathBuf;
use std::time::Duration;

use airframe_core::registry::ServiceRegistry;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::api::types::{BasicConfig, ConfigReloaded, ConfigWatcherReady};
use crate::config_listener::ConfigListenerRegistry;
use crate::io::cli::merge_from_cli;
use crate::io::env::merge_from_env_with_prefixes;
use crate::io::files::merge_toml;
use airframe_core::bus::EventBus;
use tracing::{info, instrument};

/// Spawn a file watcher task that debounces filesystem change events and reloads the
/// configuration from `src_path`, preserving defaults/env/CLI layering, then registers
/// the new BasicConfig and publishes ConfigReloaded.
///
/// Behavior mirrors the inline implementation previously in module.rs:
/// - Debounce window: 150ms of quiet coalesces multiple events into a single reload
/// - On successful reload: register BasicConfig { raw, source: Some(src_path) } and publish ConfigReloaded
/// - Respects optional CLI overrides from builder and, when feature = "args", merges argv from CliArgs
#[instrument(level = "info", skip(services, cancel, defaults_for_reload, cli_overrides_for_reload), fields(path = %src_path.display()))]
pub(crate) fn spawn_watcher(
    src_path: PathBuf,
    services: ServiceRegistry,
    cancel: tokio_util::sync::CancellationToken,
    defaults_for_reload: Option<toml::Value>,
    cli_overrides_for_reload: Option<Vec<String>>,
    env_prefixes: Vec<String>,
) {
    // Capture what we need inside the task
    let bus = services.event_bus();
    tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        // Try to create a native file watcher; fall back to polling if unavailable
        // IMPORTANT: keep the watcher alive for the duration of the task; dropping it stops events.
        let mut watcher_opt: Option<RecommendedWatcher> = None;
        if let Ok(mut w) = notify::recommended_watcher(move |_res| {
            let _ = tx.send(());
        }) {
            if w.watch(&src_path, RecursiveMode::NonRecursive).is_ok() {
                watcher_opt = Some(w);
            }
        }

        if watcher_opt.is_some() {
            // Publish watcher ready as soon as native watcher is installed
            if let Some(bus) = &bus {
                let _ = bus.publish(ConfigWatcherReady, None).await;
            }
            // Debounce: coalesce multiple fs events into a single reload after 150ms of quiet
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => { break; }
                    maybe = rx.recv() => {
                        if maybe.is_none() { break; }
                        while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await {
                            /* more events, keep waiting */
                        }
                        if let Ok(s) = std::fs::read_to_string(&src_path) {
                            if let Ok(raw_new) = s.parse::<toml::Value>() {
                                let mut merged = defaults_for_reload.clone().unwrap_or_else(|| toml::Value::Table(Default::default()));
                                merge_toml(&mut merged, raw_new);
                                merge_from_env_with_prefixes(&mut merged, &env_prefixes);
                                if let Some(overrides) = &cli_overrides_for_reload { merge_from_cli(&mut merged, overrides); }
                                #[cfg(feature = "args")]
                                if let Some(args) = services.get::<airframe_args::CliArgs>() { merge_from_cli(&mut merged, &args.argv); }
                                let cfg = BasicConfig { raw: merged, source: Some(src_path.clone()) };
                                let cfg_arc = std::sync::Arc::new(cfg);
                                services.register::<BasicConfig>(cfg_arc.clone());
                                info!(target = "airframe_config", path = %src_path.display(), "config loaded");
                                // Notify config listeners with the fresh raw config
                                if let Some(reg) = services.get::<ConfigListenerRegistry>() {
                                    let listeners = reg.all();
                                    let raw = cfg_arc.raw.clone();
                                    tokio::spawn(async move { for l in listeners.into_iter() { l.on_config_reload(&raw); } });
                                }
                                if let Some(bus) = &bus { let _ = bus.publish(ConfigReloaded, None).await; }
                            }
                        }
                    }
                }
            }
            // watcher_opt dropped here when task exits
        } else {
            // Fallback polling loop: check mtime periodically and reload on change with simple debounce
            let mut last_mod = std::fs::metadata(&src_path).and_then(|m| m.modified()).ok();
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            // Publish watcher ready for polling mode as well
            if let Some(bus) = &bus {
                let _ = bus.publish(ConfigWatcherReady, None).await;
            }
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => { break; }
                    _ = interval.tick() => {
                        let cur_mod = std::fs::metadata(&src_path).and_then(|m| m.modified()).ok();
                        if cur_mod.is_some() && cur_mod != last_mod {
                            // debounce: wait for 150ms quiet
                            tokio::time::sleep(Duration::from_millis(150)).await;
                            last_mod = cur_mod;
                            if let Ok(s) = std::fs::read_to_string(&src_path) {
                                if let Ok(raw_new) = s.parse::<toml::Value>() {
                                    let mut merged = defaults_for_reload.clone().unwrap_or_else(|| toml::Value::Table(Default::default()));
                                    merge_toml(&mut merged, raw_new);
                                    merge_from_env_with_prefixes(&mut merged, &env_prefixes);
                                    if let Some(overrides) = &cli_overrides_for_reload { merge_from_cli(&mut merged, overrides); }
                                    #[cfg(feature = "args")]
                                    if let Some(args) = services.get::<airframe_args::CliArgs>() { merge_from_cli(&mut merged, &args.argv); }
                                    let cfg = BasicConfig { raw: merged, source: Some(src_path.clone()) };
                                    let cfg_arc = std::sync::Arc::new(cfg);
                                    services.register::<BasicConfig>(cfg_arc.clone());
                                    info!(target = "airframe_config", path = %src_path.display(), "config loaded");
                                    if let Some(reg) = services.get::<ConfigListenerRegistry>() {
                                        let listeners = reg.all();
                                        let raw = cfg_arc.raw.clone();
                                        tokio::spawn(async move { for l in listeners.into_iter() { l.on_config_reload(&raw); } });
                                    }
                                    if let Some(bus) = &bus { let _ = bus.publish(ConfigReloaded, None).await; }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}
