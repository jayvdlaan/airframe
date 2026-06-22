//! Logging Module integration and runtime wiring.

#[cfg(feature = "module")]
use airframe_core::bus::EventBus;
#[cfg(feature = "module")]
use airframe_core::module::{
    Module, ModuleContext, ModuleDescriptor, CAP_ARGS, CAP_CONFIG, CAP_LOGGING,
};
#[cfg(feature = "module")]
use async_trait::async_trait;
#[cfg(feature = "module")]
use semver::Version;
#[cfg(feature = "module")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "module")]
use tokio::task::JoinHandle;
#[cfg(feature = "module")]
use tracing_subscriber::prelude::*;

#[cfg(feature = "module")]
use crate::api::config::LoggingConfig;
#[cfg(all(feature = "module", feature = "args"))]
use crate::runtime::apply_cli::apply_cli_overrides;
#[cfg(feature = "module")]
use crate::runtime::state::{LoggingControl, LoggingState};
#[cfg(feature = "module")]
use crate::{build_env_filter_from_config, build_sinks_layer, validate_sinks_first_schema};

/// Logging module implementing the Airframe Module trait.
#[cfg(feature = "module")]
pub struct LoggingModule {
    desc: ModuleDescriptor,
    watchers: Vec<JoinHandle<()>>,
    default_guard: Option<tracing::dispatcher::DefaultGuard>,
    guards_store: Option<Arc<Mutex<Vec<tracing_appender::non_blocking::WorkerGuard>>>>,
}

/// Spawn a config-sink watcher task for events that modify `LoggingConfig` and rebuild sinks.
///
/// This helper covers the common boilerplate shared by ToggleJson, SetAnsi, AddSink, RemoveSink,
/// SetSinkFilter, and SetSinkFormat watchers. For each received event it:
/// 1. Reads current `LoggingState`
/// 2. Calls the `modify` closure to mutate the config
/// 3. Rebuilds sinks via `LoggingControl`
/// 4. Persists the updated config to state
/// 5. Publishes `LoggingChanged`
/// 6. Checks cancellation
#[cfg(feature = "module")]
fn spawn_config_sink_watcher<E: airframe_core::bus::Event + 'static>(
    bus: &Arc<airframe_core::bus::inmem::InMemoryEventBus>,
    services: airframe_core::registry::ServiceRegistry,
    cancel: tokio_util::sync::CancellationToken,
    modify: impl Fn(&mut LoggingConfig, E) + Send + 'static,
) -> anyhow::Result<JoinHandle<()>> {
    use airframe_core::bus::EventBus;
    let bus_clone = bus.clone();
    let mut stream = bus_clone.subscribe::<E>()?;
    Ok(tokio::spawn(async move {
        use tokio_stream::StreamExt;
        let events = bus_clone;
        while let Some(evt) = stream.next().await {
            let Some(state) = services.get::<LoggingState>() else {
                let _ = events
                    .publish(crate::api::events::LoggingChanged, None)
                    .await;
                if cancel.is_cancelled() {
                    break;
                }
                continue;
            };
            let mut cfg = state.get();
            modify(&mut cfg, evt);
            if let Some(ctrl) = services.get::<LoggingControl>() {
                let (sinks_comp, guards) = crate::build_sinks_layer(&cfg);
                ctrl.set_sinks(sinks_comp, guards);
            }
            state.set(cfg);
            let _ = events
                .publish(crate::api::events::LoggingChanged, None)
                .await;
            if cancel.is_cancelled() {
                break;
            }
        }
    }))
}

/// Build a diagnostics vector from the sinks in a `LoggingConfig`.
#[cfg(feature = "module")]
fn build_sinks_diagnostics(cfg: &LoggingConfig) -> Vec<crate::api::events::SinkDiag> {
    let Some(ref sinks) = cfg.sinks else {
        return Vec::new();
    };
    sinks
        .iter()
        .enumerate()
        .map(|(i, s)| sink_to_diag(i, s))
        .collect()
}

/// Convert a single `SinkConfig` into its diagnostics representation.
#[cfg(feature = "module")]
fn sink_to_diag(
    sink_id: usize,
    sink: &crate::api::config::SinkConfig,
) -> crate::api::events::SinkDiag {
    match sink {
        crate::api::config::SinkConfig::Console {
            json,
            ansi,
            filter,
            format,
            ..
        } => crate::api::events::SinkDiag {
            sink_id,
            kind: "console".into(),
            path: None,
            rotation: None,
            filter: filter.clone(),
            json: *json,
            ansi: *ansi,
            with_span_events: format.as_ref().and_then(|f| f.with_span_events.clone()),
        },
        crate::api::config::SinkConfig::File {
            path,
            json,
            ansi,
            filter,
            rotation,
            format,
        } => {
            let rot_str = rotation.as_ref().map(|r| match r {
                crate::api::config::RotationConfig::Policy(p) => p.clone(),
                crate::api::config::RotationConfig::Size {
                    max_bytes, keep, ..
                } => {
                    format!("size(max_bytes={},keep={})", max_bytes, keep)
                }
            });
            crate::api::events::SinkDiag {
                sink_id,
                kind: "file".into(),
                path: Some(path.clone()),
                rotation: rot_str,
                filter: filter.clone(),
                json: *json,
                ansi: *ansi,
                with_span_events: format.as_ref().and_then(|f| f.with_span_events.clone()),
            }
        }
        crate::api::config::SinkConfig::Journald { filter } => crate::api::events::SinkDiag {
            sink_id,
            kind: "journald".into(),
            path: None,
            rotation: None,
            filter: filter.clone(),
            json: None,
            ansi: None,
            with_span_events: None,
        },
        crate::api::config::SinkConfig::Syslog { filter } => crate::api::events::SinkDiag {
            sink_id,
            kind: "syslog".into(),
            path: None,
            rotation: None,
            filter: filter.clone(),
            json: None,
            ansi: None,
            with_span_events: None,
        },
    }
}

#[cfg(feature = "module")]
impl Default for LoggingModule {
    fn default() -> Self {
        Self::new()
    }
}

impl LoggingModule {
    pub fn new() -> Self {
        // Require config only when the `config` feature is enabled; args capability is optional when feature is enabled
        #[cfg(feature = "config")]
        let requires: &[&str] = &[CAP_CONFIG.0];
        #[cfg(not(feature = "config"))]
        let requires: &[&str] = &[];
        #[cfg(feature = "args")]
        let optional_requires: &[&str] = &[CAP_ARGS.0];
        #[cfg(not(feature = "args"))]
        let optional_requires: &[&str] = &[];
        Self {
            desc: ModuleDescriptor {
                name: "logging",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[CAP_LOGGING.0],
                requires,
                optional_requires,
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
            watchers: Vec::new(),
            default_guard: None,
            guards_store: None,
        }
    }

    /// Phase 1: Load `LoggingConfig` from `BasicConfig` (if config feature) and apply CLI overrides (if args feature).
    fn load_config(&self, ctx: &ModuleContext) -> LoggingConfig {
        #[cfg(feature = "args")]
        let mut cfg: LoggingConfig = {
            #[cfg(feature = "config")]
            {
                if let Some(bc) = ctx.services.get::<airframe_config::BasicConfig>() {
                    bc.get::<LoggingConfig>("logging")
                } else {
                    LoggingConfig::default()
                }
            }
            #[cfg(not(feature = "config"))]
            {
                LoggingConfig::default()
            }
        };
        #[cfg(not(feature = "args"))]
        let cfg: LoggingConfig = {
            #[cfg(feature = "config")]
            {
                if let Some(bc) = ctx.services.get::<airframe_config::BasicConfig>() {
                    bc.get::<LoggingConfig>("logging")
                } else {
                    LoggingConfig::default()
                }
            }
            #[cfg(not(feature = "config"))]
            {
                LoggingConfig::default()
            }
        };
        #[cfg(feature = "args")]
        if let Some(args) = ctx.services.get::<airframe_args::CliArgs>() {
            apply_cli_overrides(&mut cfg, &args.argv);
        }
        cfg
    }

    /// Phase 2: Validate config, register `LoggingState`, and reflect the effective filter into state.
    fn init_state(&self, ctx: &ModuleContext, cfg: &LoggingConfig) -> anyhow::Result<LoggingState> {
        validate_sinks_first_schema(cfg)?;
        let state = LoggingState::new(cfg.clone());
        ctx.services
            .register::<LoggingState>(Arc::new(state.clone()));

        // Build initial EnvFilter and reflect the effective filter string into visible state
        let env_filter = build_env_filter_from_config(cfg);
        {
            let mut snap = state.get();
            snap.env_filter = Some(env_filter.to_string());
            // When env_filter is authoritative, clear legacy `level` for clarity
            snap.level = None;
            state.set(snap);
        }
        Ok(state)
    }

    /// Phase 3: Build reloadable filter + sinks layers, install subscriber (global or thread-local),
    /// and register `LoggingControl`.
    #[cfg(not(feature = "otel-traces"))]
    fn install_subscriber(&mut self, ctx: &ModuleContext, cfg: &LoggingConfig) {
        let env_filter = build_env_filter_from_config(cfg);
        let (filter_layer, filter_handle) = tracing_subscriber::reload::Layer::new(env_filter);

        let (sinks_comp, guards0) = build_sinks_layer(cfg);
        let (sinks_layer, sinks_handle) = tracing_subscriber::reload::Layer::new(sinks_comp);
        let subscriber = tracing_subscriber::registry()
            .with(filter_layer)
            .with(sinks_layer);
        if tracing::subscriber::set_global_default(subscriber).is_err() {
            // Fallback to thread-local default to avoid global conflicts in tests.
            // Recreate reloadable layers and keep their handles and guards.
            let (sinks_comp_local, guards_local) = build_sinks_layer(cfg);
            let (sinks_layer_local, sinks_handle_local) =
                tracing_subscriber::reload::Layer::new(sinks_comp_local);
            let (filter_layer_local, filter_handle_local) =
                tracing_subscriber::reload::Layer::new(build_env_filter_from_config(cfg));
            let local_subscriber = tracing_subscriber::registry()
                .with(filter_layer_local)
                .with(sinks_layer_local);
            let guard = tracing::dispatcher::set_default(&tracing::Dispatch::new(local_subscriber));
            self.default_guard = Some(guard);
            let guard_store = Arc::new(Mutex::new(guards_local));
            self.guards_store = Some(guard_store.clone());
            ctx.services
                .register::<LoggingControl>(Arc::new(LoggingControl {
                    filter_handle: filter_handle_local,
                    sinks_handle: sinks_handle_local,
                    file_guards: guard_store.clone(),
                }));
        } else {
            // Global installed; register control with the global handles and initial guards
            let guard_store = Arc::new(Mutex::new(guards0));
            self.guards_store = Some(guard_store.clone());
            ctx.services
                .register::<LoggingControl>(Arc::new(LoggingControl {
                    filter_handle,
                    sinks_handle,
                    file_guards: guard_store.clone(),
                }));
        }

        // Install bridge so airframe_log_api macros forward into tracing backend
        crate::install_airframe_log_api_bridge();
    }

    /// Phase 3 (with OTel tracing): Build reloadable filter + sinks + OpenTelemetry layers,
    /// install subscriber, and register `LoggingControl`.
    #[cfg(feature = "otel-traces")]
    fn install_subscriber(&mut self, ctx: &ModuleContext, cfg: &LoggingConfig) {
        let env_filter = build_env_filter_from_config(cfg);
        let (filter_layer, filter_handle) = tracing_subscriber::reload::Layer::new(env_filter);

        let (sinks_comp, guards0) = build_sinks_layer(cfg);
        let (sinks_layer, sinks_handle) = tracing_subscriber::reload::Layer::new(sinks_comp);

        let otel_layer = Self::build_otel_layer();

        let subscriber = tracing_subscriber::registry()
            .with(filter_layer)
            .with(sinks_layer)
            .with(otel_layer);
        if tracing::subscriber::set_global_default(subscriber).is_err() {
            let (sinks_comp_local, guards_local) = build_sinks_layer(cfg);
            let (sinks_layer_local, sinks_handle_local) =
                tracing_subscriber::reload::Layer::new(sinks_comp_local);
            let (filter_layer_local, filter_handle_local) =
                tracing_subscriber::reload::Layer::new(build_env_filter_from_config(cfg));
            let otel_layer_local = Self::build_otel_layer();
            let local_subscriber = tracing_subscriber::registry()
                .with(filter_layer_local)
                .with(sinks_layer_local)
                .with(otel_layer_local);
            let guard = tracing::dispatcher::set_default(&tracing::Dispatch::new(local_subscriber));
            self.default_guard = Some(guard);
            let guard_store = Arc::new(Mutex::new(guards_local));
            self.guards_store = Some(guard_store.clone());
            ctx.services
                .register::<LoggingControl>(Arc::new(LoggingControl {
                    filter_handle: filter_handle_local,
                    sinks_handle: sinks_handle_local,
                    file_guards: guard_store.clone(),
                }));
        } else {
            let guard_store = Arc::new(Mutex::new(guards0));
            self.guards_store = Some(guard_store.clone());
            ctx.services
                .register::<LoggingControl>(Arc::new(LoggingControl {
                    filter_handle,
                    sinks_handle,
                    file_guards: guard_store.clone(),
                }));
        }

        crate::install_airframe_log_api_bridge();
    }

    /// Build an optional OpenTelemetry tracing layer.
    ///
    /// Returns `Some(layer)` when `OTEL_EXPORTER_ENDPOINT` is set,
    /// `None` otherwise (the `Option<Layer>` acts as a no-op layer).
    #[cfg(feature = "otel-traces")]
    fn build_otel_layer<S>(
    ) -> Option<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        let endpoint = match std::env::var("OTEL_EXPORTER_ENDPOINT") {
            Ok(ep) if !ep.is_empty() => ep,
            _ => return None,
        };

        use opentelemetry::trace::TracerProvider;
        use opentelemetry_otlp::WithExportConfig;

        let exporter = match opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                eprintln!("airframe_logging: failed to create OTel span exporter: {e}");
                return None;
            }
        };

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                opentelemetry_sdk::Resource::builder()
                    .with_service_name("afterburner-server")
                    .build(),
            )
            .build();

        let tracer = provider.tracer("airframe_logging");

        // Set as global provider so it can be shut down later
        opentelemetry::global::set_tracer_provider(provider);

        Some(tracing_opentelemetry::OpenTelemetryLayer::new(tracer))
    }

    /// Phase 4 (config feature only): Spawn watcher for `ConfigReloaded` events to reload logging
    /// config, filter, and sinks at runtime.
    #[cfg(feature = "config")]
    fn spawn_config_reload_watcher(&mut self, ctx: &ModuleContext) -> anyhow::Result<()> {
        let Some(bus) = ctx
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        else {
            return Ok(());
        };
        let mut stream = bus.subscribe::<airframe_config::ConfigReloaded>()?;
        let services = ctx.services.clone();
        let cancel = ctx.cancel.clone();
        let bus2 = bus.clone();
        self.watchers.push(tokio::spawn(async move {
            use tokio_stream::StreamExt;
            let events = bus2;
            while let Some(_evt) = stream.next().await {
                let Some(bc) = services.get::<airframe_config::BasicConfig>() else {
                    if cancel.is_cancelled() { break; }
                    continue;
                };
                let Some(state) = services.get::<LoggingState>() else {
                    if cancel.is_cancelled() { break; }
                    continue;
                };
                #[cfg(feature = "args")]
                let mut new_cfg = bc.get::<LoggingConfig>("logging");
                #[cfg(not(feature = "args"))]
                let new_cfg = bc.get::<LoggingConfig>("logging");
                #[cfg(feature = "args")]
                if let Some(args) = services.get::<airframe_args::CliArgs>() {
                    apply_cli_overrides(&mut new_cfg, &args.argv);
                }
                // Enforce sinks-first schema; if invalid, skip applying and continue
                if let Err(e) = validate_sinks_first_schema(&new_cfg) {
                    tracing::error!(target = "airframe_logging", error = %e, "Invalid logging config: legacy keys not supported");
                    continue;
                }
                // Rebuild filter and apply via LoggingControl if available
                if let Some(ctrl) = services.get::<LoggingControl>() {
                    let new_filter = build_env_filter_from_config(&new_cfg);
                    ctrl.set_filter(new_filter);
                    // Update state visible to others including the effective env_filter string
                    let mut snap = new_cfg.clone();
                    snap.env_filter = Some(
                        ctrl.filter_handle
                            .clone_current()
                            .map(|f| f.to_string())
                            .unwrap_or_else(|| {
                                build_env_filter_from_config(&snap).to_string()
                            }),
                    );
                    snap.level = None;
                    state.set(snap);
                    // Rebuild sinks from new config and reload
                    let (sinks_comp, guards) = build_sinks_layer(&new_cfg);
                    ctrl.set_sinks(sinks_comp, guards);
                }
                let _ = events.publish(crate::api::events::LoggingChanged, None).await;
                if cancel.is_cancelled() { break; }
            }
        }));
        Ok(())
    }

    /// Phase 5: Spawn event watchers for runtime logging control commands (SetLogFilter, SetLogLevel,
    /// ToggleJson, SetAnsi, AddSink, RemoveSink, SetSinkFilter, SetSinkFormat, RequestLoggingStatus).
    fn spawn_event_watchers(&mut self, ctx: &ModuleContext) -> anyhow::Result<()> {
        let Some(bus) = ctx
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        else {
            return Ok(());
        };
        self.spawn_set_log_filter_watcher(&bus, ctx)?;
        self.spawn_set_log_level_watcher(&bus, ctx)?;
        self.spawn_sink_config_watchers(&bus, ctx)?;
        self.spawn_status_watcher(&bus, ctx)?;
        Ok(())
    }

    /// Spawn watcher for `SetLogFilter` events to change the tracing filter at runtime.
    fn spawn_set_log_filter_watcher(
        &mut self,
        bus: &Arc<airframe_core::bus::inmem::InMemoryEventBus>,
        ctx: &ModuleContext,
    ) -> anyhow::Result<()> {
        let bus_clone = bus.clone();
        let mut stream = bus_clone.subscribe::<crate::api::events::SetLogFilter>()?;
        let services = ctx.services.clone();
        let cancel = ctx.cancel.clone();
        self.watchers.push(tokio::spawn(async move {
            use tokio_stream::StreamExt;
            let events = bus_clone;
            while let Some(evt) = stream.next().await {
                if let Some(ctrl) = services.get::<LoggingControl>() {
                    let new_filter = tracing_subscriber::EnvFilter::new(evt.filter.clone());
                    ctrl.set_filter(new_filter);
                }
                if let Some(state) = services.get::<LoggingState>() {
                    let mut cfg = state.get();
                    cfg.env_filter = Some(evt.filter.clone());
                    cfg.level = None;
                    state.set(cfg);
                }
                let _ = events
                    .publish(crate::api::events::LoggingChanged, None)
                    .await;
                if cancel.is_cancelled() {
                    break;
                }
            }
        }));
        Ok(())
    }

    /// Spawn watcher for `SetLogLevel` events to change default or per-target level at runtime.
    fn spawn_set_log_level_watcher(
        &mut self,
        bus: &Arc<airframe_core::bus::inmem::InMemoryEventBus>,
        ctx: &ModuleContext,
    ) -> anyhow::Result<()> {
        let bus_clone = bus.clone();
        let mut stream = bus_clone.subscribe::<crate::api::events::SetLogLevel>()?;
        let services = ctx.services.clone();
        let cancel = ctx.cancel.clone();
        self.watchers.push(tokio::spawn(async move {
            use tokio_stream::StreamExt;
            let events = bus_clone;
            while let Some(evt) = stream.next().await {
                // Construct a new filter string by prepending the new directive to the existing one (first match wins)
                let existing = if let Some(state) = services.get::<LoggingState>() {
                    let cfg = state.get();
                    cfg.env_filter.clone().unwrap_or_else(|| {
                        // if none, build from current cfg
                        let f = crate::build_env_filter_from_config(&cfg);
                        f.to_string()
                    })
                } else {
                    String::from("info")
                };
                let directive = if let Some(t) = &evt.target {
                    format!("{}={}", t, evt.level)
                } else {
                    evt.level.clone()
                };
                let new_spec = if existing.is_empty() {
                    directive.clone()
                } else {
                    format!("{},{}", directive, existing)
                };
                if let Some(ctrl) = services.get::<LoggingControl>() {
                    let new_filter = tracing_subscriber::EnvFilter::new(new_spec.clone());
                    ctrl.set_filter(new_filter);
                }
                if let Some(state) = services.get::<LoggingState>() {
                    let mut cfg = state.get();
                    cfg.env_filter = Some(new_spec);
                    cfg.level = None;
                    state.set(cfg);
                }
                let _ = events
                    .publish(crate::api::events::LoggingChanged, None)
                    .await;
                if cancel.is_cancelled() {
                    break;
                }
            }
        }));
        Ok(())
    }

    /// Spawn watchers for sink configuration events: ToggleJson, SetAnsi, AddSink, RemoveSink,
    /// SetSinkFilter, and SetSinkFormat.
    fn spawn_sink_config_watchers(
        &mut self,
        bus: &Arc<airframe_core::bus::inmem::InMemoryEventBus>,
        ctx: &ModuleContext,
    ) -> anyhow::Result<()> {
        // Listen for ToggleJson to update formatting preference in state
        self.watchers
            .push(spawn_config_sink_watcher::<crate::api::events::ToggleJson>(
                bus,
                ctx.services.clone(),
                ctx.cancel.clone(),
                |cfg, evt| {
                    cfg.json = Some(evt.enabled);
                },
            )?);
        // Listen for SetAnsi to update ANSI flag in state
        self.watchers
            .push(spawn_config_sink_watcher::<crate::api::events::SetAnsi>(
                bus,
                ctx.services.clone(),
                ctx.cancel.clone(),
                |cfg, evt| {
                    cfg.ansi = Some(evt.enabled);
                },
            )?);
        // Runtime sink control: AddSink
        self.watchers
            .push(spawn_config_sink_watcher::<crate::api::events::AddSink>(
                bus,
                ctx.services.clone(),
                ctx.cancel.clone(),
                |cfg, evt| {
                    let mut sinks = cfg.sinks.take().unwrap_or_default();
                    sinks.push(evt.sink);
                    cfg.sinks = Some(sinks);
                },
            )?);
        // RemoveSink
        self.watchers
            .push(spawn_config_sink_watcher::<crate::api::events::RemoveSink>(
                bus,
                ctx.services.clone(),
                ctx.cancel.clone(),
                |cfg, evt| {
                    if let Some(ref mut sinks) = cfg.sinks {
                        if evt.sink_id < sinks.len() {
                            sinks.remove(evt.sink_id);
                        }
                    }
                },
            )?);
        // SetSinkFilter
        self.watchers.push(spawn_config_sink_watcher::<
            crate::api::events::SetSinkFilter,
        >(
            bus,
            ctx.services.clone(),
            ctx.cancel.clone(),
            |cfg, evt| {
                let Some(sink) = cfg.sinks.as_mut().and_then(|s| s.get_mut(evt.sink_id)) else {
                    return;
                };
                match sink {
                    crate::api::config::SinkConfig::Console { filter, .. } => {
                        *filter = evt.filter.clone();
                    }
                    crate::api::config::SinkConfig::File { filter, .. } => {
                        *filter = evt.filter.clone();
                    }
                    crate::api::config::SinkConfig::Journald { filter } => {
                        *filter = evt.filter.clone();
                    }
                    crate::api::config::SinkConfig::Syslog { .. } => { /* no-op for now */ }
                }
            },
        )?);
        // SetSinkFormat
        self.watchers.push(spawn_config_sink_watcher::<
            crate::api::events::SetSinkFormat,
        >(
            bus,
            ctx.services.clone(),
            ctx.cancel.clone(),
            |cfg, evt| {
                let Some(sink) = cfg.sinks.as_mut().and_then(|s| s.get_mut(evt.sink_id)) else {
                    return;
                };
                match sink {
                    crate::api::config::SinkConfig::Console {
                        json, ansi, format, ..
                    } => {
                        if let Some(b) = evt.json {
                            *json = Some(b);
                        }
                        if let Some(b) = evt.ansi {
                            *ansi = Some(b);
                        }
                        if let Some(span) = evt.with_span_events.clone() {
                            format.get_or_insert(Default::default()).with_span_events = Some(span);
                        }
                        // console sink ignores correlation ID flag
                    }
                    crate::api::config::SinkConfig::File {
                        json, ansi, format, ..
                    } => {
                        if let Some(b) = evt.json {
                            *json = Some(b);
                        }
                        if let Some(b) = evt.ansi {
                            *ansi = Some(b);
                        }
                        if let Some(span) = evt.with_span_events.clone() {
                            format.get_or_insert(Default::default()).with_span_events = Some(span);
                        }
                        if let Some(corr) = evt.include_correlation_id {
                            format
                                .get_or_insert(Default::default())
                                .include_correlation_id = Some(corr);
                        }
                    }
                    crate::api::config::SinkConfig::Journald { .. }
                    | crate::api::config::SinkConfig::Syslog { .. } => {}
                }
            },
        )?);
        Ok(())
    }

    /// Spawn watcher for `RequestLoggingStatus` events to respond with a diagnostics snapshot
    /// of current logging configuration and sink state.
    fn spawn_status_watcher(
        &mut self,
        bus: &Arc<airframe_core::bus::inmem::InMemoryEventBus>,
        ctx: &ModuleContext,
    ) -> anyhow::Result<()> {
        let bus_clone = bus.clone();
        let mut stream = bus_clone.subscribe::<crate::api::events::RequestLoggingStatus>()?;
        let services = ctx.services.clone();
        let cancel = ctx.cancel.clone();
        self.watchers.push(tokio::spawn(async move {
            use tokio_stream::StreamExt;
            let events = bus_clone;
            while let Some(_evt) = stream.next().await {
                let Some(state) = services.get::<LoggingState>() else {
                    if cancel.is_cancelled() {
                        break;
                    }
                    continue;
                };
                let cfg = state.get();
                let global_filter = Some(crate::build_env_filter_from_config(&cfg).to_string());
                let sinks_diag = build_sinks_diagnostics(&cfg);
                let status = crate::api::events::LoggingStatus {
                    config: cfg,
                    global_filter,
                    sinks: sinks_diag,
                };
                let _ = events.publish(status, None).await;
                if cancel.is_cancelled() {
                    break;
                }
            }
        }));
        Ok(())
    }
}

#[cfg(feature = "module")]
#[async_trait]
impl Module for LoggingModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        // Phase 1: Load and merge config
        let cfg = self.load_config(&ctx);

        // Phase 2: Validate config, register LoggingState, reflect effective filter
        let _state = self.init_state(&ctx, &cfg)?;

        // Phase 3: Build subscriber layers, install global/thread-local, register LoggingControl
        self.install_subscriber(&ctx, &cfg);

        // Emit a startup log so users can verify logging is active and see where it goes
        tracing::info!(target = "airframe_logging", directives = ?cfg.directives, sinks = ?cfg.sinks, "logging initialized");

        // Phase 4: Spawn config reload watcher (config feature only)
        #[cfg(feature = "config")]
        self.spawn_config_reload_watcher(&ctx)?;

        // Phase 5: Spawn event watchers for runtime logging control
        self.spawn_event_watchers(&ctx)?;

        Ok(())
    }
}
