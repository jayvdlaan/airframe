//! File sink construction helpers.

use crate::api::config::{FormatOptions, RotationConfig};
use crate::io::correlation_json_writer::CorrelationJsonWriter;
use crate::io::rotation::SizeRollingFile;
use crate::layer_parent::ParentSubscriber;
use crate::sinks::format_helpers::{
    apply_all_format_options, apply_format_options, apply_span_events,
};

/// Build a file fmt layer with non-blocking writer and rotation.
/// Returns the layer and the worker guard that must be kept alive.
pub fn build_file_layer(
    path: &str,
    json: bool,
    ansi: bool,
    rotation: Option<&RotationConfig>,
    format: Option<&FormatOptions>,
    non_blocking_buffer_lines: Option<usize>,
    include_correlation: bool,
) -> (
    Box<dyn tracing_subscriber::Layer<ParentSubscriber> + Send + Sync>,
    tracing_appender::non_blocking::WorkerGuard,
) {
    let p = std::path::Path::new(path);
    let (dir_path, file_name) = match (p.parent(), p.file_name()) {
        (Some(d), Some(f)) => (d.to_path_buf(), f.to_string_lossy().to_string()),
        _ => {
            let cur = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            (
                cur.clone(),
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            )
        }
    };
    let _ = std::fs::create_dir_all(&dir_path);

    enum AppenderChoice {
        Std(tracing_appender::rolling::RollingFileAppender),
        Custom(Box<dyn std::io::Write + Send + 'static>),
    }
    let app_choice: AppenderChoice = match rotation {
        Some(RotationConfig::Policy(r)) => {
            let r = r.to_ascii_lowercase();
            if r == "daily" {
                AppenderChoice::Std(tracing_appender::rolling::daily(&dir_path, &file_name))
            } else if r == "hourly" {
                AppenderChoice::Std(tracing_appender::rolling::hourly(&dir_path, &file_name))
            } else {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path);
                AppenderChoice::Std(tracing_appender::rolling::never(&dir_path, &file_name))
            }
        }
        Some(RotationConfig::Size {
            policy,
            max_bytes,
            keep,
        }) => {
            if policy.eq_ignore_ascii_case("size") {
                let w =
                    SizeRollingFile::new(dir_path.clone(), file_name.clone(), *max_bytes, *keep)
                        .unwrap();
                AppenderChoice::Custom(Box::new(w))
            } else {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path);
                AppenderChoice::Std(tracing_appender::rolling::never(&dir_path, &file_name))
            }
        }
        None => {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path);
            AppenderChoice::Std(tracing_appender::rolling::never(&dir_path, &file_name))
        }
    };

    let mut builder = tracing_appender::non_blocking::NonBlockingBuilder::default();
    if let Some(lines) = non_blocking_buffer_lines {
        builder = builder.buffered_lines_limit(lines);
    }
    let (nb, guard) = match app_choice {
        AppenderChoice::Std(app) => builder.finish(app),
        AppenderChoice::Custom(w) => builder.finish(w),
    };

    if json {
        // Use correlation-aware writer when enabled to match crate behavior
        let nb_clone = nb.clone();
        let make_writer = move || CorrelationJsonWriter::new(nb_clone.clone(), include_correlation);
        let mut base = tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(ansi)
            .with_writer(make_writer);
        if let Some(fmt) = format {
            apply_all_format_options!(base, fmt);
        }
        (Box::new(base), guard)
    } else {
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
        (Box::new(base), guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    fn init_with_layer(
        layer: Box<dyn tracing_subscriber::Layer<ParentSubscriber> + Send + Sync>,
    ) -> tracing::dispatcher::DefaultGuard {
        // ParentSubscriber is a Registry layered with a reloadable EnvFilter
        let (filter_layer, _handle) =
            tracing_subscriber::reload::Layer::new(tracing_subscriber::EnvFilter::new("info"));
        let subscriber: ParentSubscriber =
            tracing_subscriber::registry::Registry::default().with(filter_layer);
        let subscriber = subscriber.with(layer);
        tracing::subscriber::set_default(subscriber)
    }

    #[test]
    fn builds_and_writes_plain_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        let (layer, guard) = build_file_layer(
            path.to_str().unwrap(),
            false,
            false,
            None,
            None,
            None,
            false,
        );
        let _g = init_with_layer(layer);
        // Keep the non-blocking guard alive
        let _guard = guard;
        tracing::info!(target = "airframe_logging", "hello file plain");
        // Allow some time for the non-blocking writer to flush
        std::thread::sleep(std::time::Duration::from_millis(200));
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            content.contains("hello file plain"),
            "file should contain log message; got: {}",
            content
        );
    }

    #[test]
    fn builds_and_writes_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.jsonl");
        let (layer, guard) = build_file_layer(
            path.to_str().unwrap(),
            true,
            false,
            None,
            None,
            Some(1024),
            true,
        );
        let _g = init_with_layer(layer);
        // Keep the non-blocking guard alive
        let _guard = guard;
        tracing::info!(target = "airframe_logging", msg = "hello file json");
        std::thread::sleep(std::time::Duration::from_millis(200));
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        // Expect JSON-looking content
        assert!(content.contains("hello file json") || content.contains('{'));
    }
}
