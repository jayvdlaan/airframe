//! Console sink construction helpers.

use crate::api::config::FormatOptions;
use crate::layer_parent::ParentSubscriber;
use crate::sinks::format_helpers::{
    apply_all_format_options, apply_format_options, apply_span_events,
};

/// Build a console fmt layer, optionally JSON, choosing stdout/stderr.
pub fn build_console_layer(
    json: bool,
    ansi: bool,
    format: Option<&FormatOptions>,
    to_stderr: bool,
) -> Box<dyn tracing_subscriber::Layer<ParentSubscriber> + Send + Sync> {
    if json {
        if to_stderr {
            let mut l = tracing_subscriber::fmt::layer()
                .json()
                .with_ansi(ansi)
                .with_writer(std::io::stderr);
            if let Some(fmt) = format {
                apply_all_format_options!(l, fmt);
            }
            Box::new(l)
        } else {
            let mut l = tracing_subscriber::fmt::layer().json().with_ansi(ansi);
            if let Some(fmt) = format {
                apply_all_format_options!(l, fmt);
            }
            Box::new(l)
        }
    } else {
        let mut fmt_cfg = tracing_subscriber::fmt::format();
        if let Some(fmt) = format {
            apply_format_options!(fmt_cfg, fmt);
        }
        if to_stderr {
            let base0 = tracing_subscriber::fmt::layer()
                .with_ansi(ansi)
                .with_writer(std::io::stderr);
            let mut l = base0.event_format(fmt_cfg);
            if let Some(fmt) = format {
                apply_span_events!(l, fmt);
            }
            Box::new(l)
        } else {
            let base0 = tracing_subscriber::fmt::layer().with_ansi(ansi);
            let mut l = base0.event_format(fmt_cfg);
            if let Some(fmt) = format {
                apply_span_events!(l, fmt);
            }
            Box::new(l)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::config::FormatOptions;
    use tracing_subscriber::prelude::*;

    fn with_subscriber(layer: Box<dyn tracing_subscriber::Layer<ParentSubscriber> + Send + Sync>) {
        let env = tracing_subscriber::EnvFilter::new("info");
        // Build a reloadable filter layer so the subscriber type matches ParentSubscriber
        let (filter_layer, _handle) = tracing_subscriber::reload::Layer::new(env);
        let subscriber = tracing_subscriber::registry::Registry::default()
            .with(filter_layer)
            .with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);
        tracing::info!(target: "airframe_logging", "hello console layer");
    }

    #[test]
    fn builds_plain_stdout_with_options() {
        let fmt = FormatOptions {
            target: Some(true),
            level: Some(true),
            thread: Some(true),
            file: Some(false),
            line: Some(false),
            with_span_events: Some("enter".into()),
            ..Default::default()
        };
        let layer = build_console_layer(false, false, Some(&fmt), false);
        with_subscriber(layer);
    }

    #[test]
    fn builds_json_stderr_with_span_events() {
        let fmt = FormatOptions {
            with_span_events: Some("full".into()),
            ..Default::default()
        };
        let layer = build_console_layer(true, true, Some(&fmt), true);
        with_subscriber(layer);
    }
}
