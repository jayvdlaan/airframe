//! airframe_log_api: Minimal logging API for Airframe
//!
//! - Define a `Logger` trait to receive log records.
//! - Provide a global `set_logger` to install a process-wide logger.
//! - Provide macros (`error!`, `warn!`, `info!`, `debug!`, `trace!`) that are
//!   no-ops when no logger is installed and do not evaluate their arguments.

use core::fmt;
use std::sync::OnceLock;

/// Logging level.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Logger trait that backends can implement to receive log messages.
pub trait Logger: Sync + Send {
    /// Process a log message at the provided level.
    ///
    /// Implementations should be fast and non-panicking.
    fn log(&self, level: Level, args: &fmt::Arguments);

    /// Optional: check if the given level is enabled. Default enables all.
    fn enabled(&self, _level: Level) -> bool {
        true
    }
}

static LOGGER: OnceLock<&'static dyn Logger> = OnceLock::new();

/// Error returned when a logger is already set.
#[derive(Debug)]
pub struct SetLoggerError;

/// Install a global logger. Can only be set once.
pub fn set_logger(logger: &'static dyn Logger) -> Result<(), SetLoggerError> {
    LOGGER.set(logger).map_err(|_| SetLoggerError)
}

/// Returns true if a global logger has been installed.
pub fn is_enabled() -> bool {
    LOGGER.get().is_some()
}

/// Log using the installed global logger if present and enabled for the level.
pub fn log(level: Level, args: fmt::Arguments) {
    if let Some(logger) = LOGGER.get() {
        if logger.enabled(level) {
            logger.log(level, &args);
        }
    }
}

/// Internal helper to implement per-level macros without evaluating arguments
/// when no global logger is set.
#[doc(hidden)]
#[inline(always)]
pub fn __log_if_enabled(level: Level, args: fmt::Arguments) {
    log(level, args)
}

/// Log a message at ERROR level. No-ops if no logger installed.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        if $crate::is_enabled() {
            $crate::__log_if_enabled($crate::Level::Error, format_args!($($arg)*));
        }
    }};
}

/// Log a message at WARN level. No-ops if no logger installed.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        if $crate::is_enabled() {
            $crate::__log_if_enabled($crate::Level::Warn, format_args!($($arg)*));
        }
    }};
}

/// Log a message at INFO level. No-ops if no logger installed.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        if $crate::is_enabled() {
            $crate::__log_if_enabled($crate::Level::Info, format_args!($($arg)*));
        }
    }};
}

/// Log a message at DEBUG level. No-ops if no logger installed.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        if $crate::is_enabled() {
            $crate::__log_if_enabled($crate::Level::Debug, format_args!($($arg)*));
        }
    }};
}

/// Log a message at TRACE level. No-ops if no logger installed.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{
        if $crate::is_enabled() {
            $crate::__log_if_enabled($crate::Level::Trace, format_args!($($arg)*));
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A simple test logger to validate that logging works when installed.
    struct CountingLogger<'a> {
        counter: &'a AtomicUsize,
    }

    impl<'a> Logger for CountingLogger<'a> {
        fn log(&self, _level: Level, _args: &fmt::Arguments) {
            self.counter.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn no_logger_is_no_op_and_does_not_evaluate_arguments() {
        // Skip if logger already set by another test (test order is non-deterministic)
        if is_enabled() {
            return;
        }

        static SIDE_EFFECTS: AtomicUsize = AtomicUsize::new(0);

        // None of these should evaluate the arguments or change SIDE_EFFECTS
        error!("error: {}", SIDE_EFFECTS.fetch_add(1, Ordering::SeqCst));
        warn!("warn: {}", SIDE_EFFECTS.fetch_add(1, Ordering::SeqCst));
        info!("info: {}", SIDE_EFFECTS.fetch_add(1, Ordering::SeqCst));
        debug!("debug: {}", SIDE_EFFECTS.fetch_add(1, Ordering::SeqCst));
        trace!("trace: {}", SIDE_EFFECTS.fetch_add(1, Ordering::SeqCst));

        assert_eq!(
            SIDE_EFFECTS.load(Ordering::SeqCst),
            0,
            "no-op macros must not evaluate arguments when no logger is set"
        );
    }

    #[test]
    fn set_logger_enables_logging() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);

        // Install logger once per process. This test assumes it's the first to set it.
        // If set_logger has already been called, skip this test gracefully.
        if set_logger(Box::leak(Box::new(CountingLogger { counter: &COUNT })) as &'static dyn Logger).is_ok() {
            let before = COUNT.load(Ordering::SeqCst);
            info!("hello {}", 1);
            debug!("world {}", 2);
            error!("! {}", 3);
            let after = COUNT.load(Ordering::SeqCst);
            assert!(after >= before + 3, "logger should have been called for each macro invocation");
        }
    }
}
