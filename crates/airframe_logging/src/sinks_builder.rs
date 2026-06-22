use crate::api::config::{FormatOptions, LoggingConfig, RotationConfig, SinkConfig};
use crate::filters::per_sink::PerSinkFilter;
use crate::io::rotation::SizeRollingFile;
use crate::layer_parent::ParentSubscriber;
use crate::layers::sinks_layer::{SinkEntry, SinksLayer};
use crate::sinks::format_helpers::{
    apply_all_format_options, apply_format_options, apply_span_events,
};

type BoxedLayer = Box<dyn tracing_subscriber::Layer<ParentSubscriber> + Send + Sync>;

pub(crate) fn build_sinks_layer(
    cfg: &LoggingConfig,
) -> (SinksLayer, Vec<tracing_appender::non_blocking::WorkerGuard>) {
    let mut layers: Vec<SinkEntry> = Vec::new();
    let mut guards: Vec<tracing_appender::non_blocking::WorkerGuard> = Vec::new();

    // Global overrides for legacy top-level flags
    let override_json = cfg.json;
    let override_ansi = cfg.ansi;

    let sinks = cfg.sinks.as_deref().unwrap_or_default();

    for sink in sinks {
        match sink {
            SinkConfig::Console {
                json,
                ansi,
                filter,
                format,
                stderr,
            } => {
                build_console_sink(
                    override_json,
                    override_ansi,
                    json,
                    ansi,
                    filter,
                    format,
                    stderr,
                    &mut layers,
                );
            }
            SinkConfig::File {
                path,
                json,
                ansi,
                filter,
                rotation,
                format,
            } => {
                build_file_sink(
                    cfg,
                    override_json,
                    override_ansi,
                    path,
                    json,
                    ansi,
                    filter,
                    rotation,
                    format,
                    &mut layers,
                    &mut guards,
                );
            }
            SinkConfig::Journald { filter: _filter } => {
                build_journald_sink(_filter, &mut layers);
            }
            SinkConfig::Syslog { filter: _filter } => {
                build_syslog_sink(_filter, &mut layers);
            }
        }
    }

    // Fallback: if no sinks were configured, install a default console sink to stderr
    if layers.is_empty() {
        let layer_box: BoxedLayer =
            crate::sinks::console::build_console_layer(false, true, None, true);
        layers.push(SinkEntry {
            layer: layer_box,
            filter: None,
        });
    }

    (SinksLayer::new(layers), guards)
}

// ---------------------------------------------------------------------------
// Console sink
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_console_sink(
    override_json: Option<bool>,
    override_ansi: Option<bool>,
    json: &Option<bool>,
    ansi: &Option<bool>,
    filter: &Option<String>,
    format: &Option<FormatOptions>,
    stderr: &Option<bool>,
    layers: &mut Vec<SinkEntry>,
) {
    let j = override_json.unwrap_or(json.unwrap_or(false));
    let a = override_ansi.unwrap_or(ansi.unwrap_or(true));
    let to_stderr = stderr.unwrap_or(false);
    let layer_box: BoxedLayer =
        crate::sinks::console::build_console_layer(j, a, format.as_ref(), to_stderr);
    let parsed = filter.as_ref().map(|s| PerSinkFilter::parse(s));
    layers.push(SinkEntry {
        layer: layer_box,
        filter: parsed,
    });
}

// ---------------------------------------------------------------------------
// File sink
// ---------------------------------------------------------------------------

/// Intermediate enum to unify standard rolling appenders and custom writers.
enum AppenderChoice {
    Std(tracing_appender::rolling::RollingFileAppender),
    Custom(Box<dyn std::io::Write + Send + 'static>),
}

#[allow(clippy::too_many_arguments)]
fn build_file_sink(
    cfg: &LoggingConfig,
    override_json: Option<bool>,
    override_ansi: Option<bool>,
    path: &str,
    json: &Option<bool>,
    ansi: &Option<bool>,
    filter: &Option<String>,
    rotation: &Option<RotationConfig>,
    format: &Option<FormatOptions>,
    layers: &mut Vec<SinkEntry>,
    guards: &mut Vec<tracing_appender::non_blocking::WorkerGuard>,
) {
    let j = override_json.unwrap_or(json.unwrap_or(false));
    let a = override_ansi.unwrap_or(ansi.unwrap_or(false));
    let p = std::path::Path::new(path);

    let file_name = match p.file_name() {
        Some(fname) => fname.to_string_lossy().to_string(),
        None => {
            tracing::warn!(target = "airframe_logging", path = %path, "file sink path has no file name; skipping");
            return;
        }
    };

    let dir_path: std::path::PathBuf = p
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    let _ = std::fs::create_dir_all(&dir_path);

    let app_choice = resolve_appender(rotation, path, &dir_path, &file_name);

    let mut builder = tracing_appender::non_blocking::NonBlockingBuilder::default();
    if let Some(lines) = cfg.non_blocking_buffer_lines {
        builder = builder.buffered_lines_limit(lines);
    }
    let (nb, guard) = match app_choice {
        AppenderChoice::Std(app) => builder.finish(app),
        AppenderChoice::Custom(w) => builder.finish(w),
    };
    guards.push(guard);

    let layer_box: BoxedLayer = if j {
        build_file_json_layer(nb, a, format)
    } else {
        build_file_text_layer(nb, a, format)
    };

    let parsed = filter.as_ref().map(|s| PerSinkFilter::parse(s));
    layers.push(SinkEntry {
        layer: layer_box,
        filter: parsed,
    });
}

/// Choose the appender based on the rotation policy.
fn resolve_appender(
    rotation: &Option<RotationConfig>,
    path: &str,
    dir_path: &std::path::Path,
    file_name: &str,
) -> AppenderChoice {
    match rotation {
        Some(RotationConfig::Policy(p)) => resolve_policy_appender(p, path, dir_path, file_name),
        Some(RotationConfig::Size {
            policy,
            max_bytes,
            keep,
        }) => resolve_size_appender(policy, *max_bytes, *keep, path, dir_path, file_name),
        None => never_appender(path, dir_path, file_name),
    }
}

fn resolve_policy_appender(
    policy: &str,
    path: &str,
    dir_path: &std::path::Path,
    file_name: &str,
) -> AppenderChoice {
    let r = policy.to_ascii_lowercase();
    match r.as_str() {
        "daily" => AppenderChoice::Std(tracing_appender::rolling::daily(dir_path, file_name)),
        "hourly" => AppenderChoice::Std(tracing_appender::rolling::hourly(dir_path, file_name)),
        _ => never_appender(path, dir_path, file_name),
    }
}

fn resolve_size_appender(
    policy: &str,
    max_bytes: u64,
    keep: usize,
    path: &str,
    dir_path: &std::path::Path,
    file_name: &str,
) -> AppenderChoice {
    if !policy.eq_ignore_ascii_case("size") {
        return never_appender(path, dir_path, file_name);
    }

    match SizeRollingFile::new(
        dir_path.to_path_buf(),
        file_name.to_string(),
        max_bytes,
        keep,
    ) {
        Ok(w) => AppenderChoice::Custom(Box::new(w)),
        Err(e) => {
            tracing::warn!(target = "airframe_logging", error = %e, path = %path, "failed to initialize size-rolling file sink; falling back to never-rotating appender");
            never_appender(path, dir_path, file_name)
        }
    }
}

/// Create a non-rotating appender, touching the file first.
fn never_appender(path: &str, dir_path: &std::path::Path, file_name: &str) -> AppenderChoice {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path);
    AppenderChoice::Std(tracing_appender::rolling::never(dir_path, file_name))
}

// ---------------------------------------------------------------------------
// File format layer builders (JSON vs text)
// ---------------------------------------------------------------------------

fn build_file_json_layer(
    nb: tracing_appender::non_blocking::NonBlocking,
    ansi: bool,
    format: &Option<FormatOptions>,
) -> BoxedLayer {
    let enabled = format
        .as_ref()
        .and_then(|f| f.include_correlation_id)
        .unwrap_or(true);
    let nb_clone = nb.clone();
    let make_writer = move || {
        crate::io::correlation_json_writer::CorrelationJsonWriter::new(nb_clone.clone(), enabled)
    };
    let mut base = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(ansi)
        .with_writer(make_writer);

    if let Some(fmt) = format {
        apply_all_format_options!(base, fmt);
    }

    Box::new(base)
}

fn build_file_text_layer(
    nb: tracing_appender::non_blocking::NonBlocking,
    ansi: bool,
    format: &Option<FormatOptions>,
) -> BoxedLayer {
    let mut fmt_cfg = tracing_subscriber::fmt::format();

    if let Some(fmt) = format {
        apply_format_options!(fmt_cfg, fmt);
    }

    let mut base = tracing_subscriber::fmt::layer()
        .with_ansi(ansi)
        .with_writer(nb)
        .event_format(fmt_cfg);

    if let Some(fmt) = format {
        apply_span_events!(base, fmt);
    }

    Box::new(base)
}

// ---------------------------------------------------------------------------
// Journald sink
// ---------------------------------------------------------------------------

fn build_journald_sink(_filter: &Option<String>, layers: &mut Vec<SinkEntry>) {
    #[cfg(feature = "adapters-journald")]
    {
        match tracing_journald::layer() {
            Ok(l) => {
                let parsed = _filter.as_ref().map(|s| PerSinkFilter::parse(s));
                let layer_box: BoxedLayer = Box::new(l);
                layers.push(SinkEntry {
                    layer: layer_box,
                    filter: parsed,
                });
            }
            Err(e) => {
                tracing::warn!(target = "airframe_logging", error = %e, "failed to initialize journald layer; skipping");
            }
        }
    }
    #[cfg(not(feature = "adapters-journald"))]
    {
        let _ = (_filter, layers);
        tracing::warn!(
            target = "airframe_logging",
            "journald sink requested but feature not enabled; skipping"
        );
    }
}

// ---------------------------------------------------------------------------
// Syslog sink
// ---------------------------------------------------------------------------

fn build_syslog_sink(_filter: &Option<String>, layers: &mut Vec<SinkEntry>) {
    #[cfg(feature = "adapters-syslog")]
    {
        use std::borrow::Cow;
        let ident = Cow::Borrowed(c"airframe");
        let (options, facility) = Default::default();
        let Some(writer) = syslog_tracing::Syslog::new(ident, options, facility) else {
            tracing::warn!(
                target = "airframe_logging",
                "failed to initialize syslog writer; skipping"
            );
            return;
        };
        let base = tracing_subscriber::fmt::layer().with_writer(writer);
        let parsed = _filter.as_ref().map(|s| PerSinkFilter::parse(s));
        let layer_box: BoxedLayer = Box::new(base);
        layers.push(SinkEntry {
            layer: layer_box,
            filter: parsed,
        });
    }
    #[cfg(not(feature = "adapters-syslog"))]
    {
        let _ = (_filter, layers);
        tracing::warn!(
            target = "airframe_logging",
            "syslog sink requested but feature not enabled; skipping"
        );
    }
}

#[cfg(all(test, feature = "adapters-syslog", unix))]
mod tests_syslog {
    use crate::api::config::{LoggingConfig, SinkConfig};
    use crate::sinks_builder::build_sinks_layer;

    #[test]
    fn syslog_sink_builds_layer() {
        let cfg = LoggingConfig {
            directives: Some(vec!["info".into()]),
            sinks: Some(vec![SinkConfig::Syslog { filter: None }]),
            ..Default::default()
        };
        let (_layer, _guards) = build_sinks_layer(&cfg);
    }
}
