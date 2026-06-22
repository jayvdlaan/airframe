//! Shared helpers for applying [`FormatOptions`] to tracing fmt layers and format configs.
//!
//! The tracing-subscriber `fmt::Layer` and `fmt::format::Format` types expose the same
//! builder methods (`.with_target()`, `.with_level()`, etc.) but do **not** share a common
//! trait. We use macros to eliminate the duplicated option-application blocks that previously
//! appeared in `console.rs`, `file.rs`, and `sinks_builder.rs`.

use tracing_subscriber::fmt::format::FmtSpan;

/// Parse a span-events string into the corresponding [`FmtSpan`] value.
pub(crate) fn parse_span_events(s: &str) -> FmtSpan {
    match s {
        "none" => FmtSpan::NONE,
        "new" => FmtSpan::NEW,
        "enter" => FmtSpan::ENTER,
        "full" => FmtSpan::FULL,
        _ => FmtSpan::NONE,
    }
}

/// Apply the common subset of [`FormatOptions`] (target, level, thread, file, line) to any
/// builder that exposes the corresponding `with_*` methods.
///
/// This works for both `tracing_subscriber::fmt::Layer` (all variants) **and**
/// `tracing_subscriber::fmt::format::Format`.
///
/// # Usage
/// ```ignore
/// apply_format_options!(layer, &format_options);
/// ```
macro_rules! apply_format_options {
    ($builder:ident, $fmt:expr) => {
        if let Some(v) = $fmt.target {
            $builder = $builder.with_target(v);
        }
        if let Some(v) = $fmt.level {
            $builder = $builder.with_level(v);
        }
        if let Some(true) = $fmt.thread {
            $builder = $builder.with_thread_ids(true).with_thread_names(true);
        }
        if let Some(v) = $fmt.file {
            $builder = $builder.with_file(v);
        }
        if let Some(v) = $fmt.line {
            $builder = $builder.with_line_number(v);
        }
    };
}

/// Apply span-events from [`FormatOptions`] to any builder that has `.with_span_events()`.
///
/// This is separate from [`apply_format_options!`] because `fmt::format::Format` does **not**
/// have a `with_span_events` method -- only `fmt::Layer` does.
///
/// # Usage
/// ```ignore
/// apply_span_events!(layer, &format_options);
/// ```
macro_rules! apply_span_events {
    ($builder:ident, $fmt:expr) => {
        if let Some(fs) = $fmt
            .with_span_events
            .as_ref()
            .map(|s| $crate::sinks::format_helpers::parse_span_events(s))
        {
            $builder = $builder.with_span_events(fs);
        }
    };
}

/// Convenience macro that applies **all** format options including span events.
///
/// Use this for `fmt::Layer` types (JSON or plain-with-writer) that support every option.
/// For `fmt::format::Format` types, use [`apply_format_options!`] alone (they lack
/// `with_span_events`).
macro_rules! apply_all_format_options {
    ($builder:ident, $fmt:expr) => {
        apply_format_options!($builder, $fmt);
        apply_span_events!($builder, $fmt);
    };
}

pub(crate) use apply_all_format_options;
pub(crate) use apply_format_options;
pub(crate) use apply_span_events;
