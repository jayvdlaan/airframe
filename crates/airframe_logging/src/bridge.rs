// Bridge: implement airframe_log_api::Logger by forwarding to tracing
// This allows crates using the airframe_log_api macros to emit events through
// the tracing backend configured by airframe_logging.
pub struct TracingLogger;

static TRACING_LOGGER: TracingLogger = TracingLogger;

impl airframe_log_api::Logger for TracingLogger {
    fn log(&self, level: airframe_log_api::Level, args: &core::fmt::Arguments) {
        // Forward as a tracing event with fixed target to distinguish bridge logs.
        match level {
            airframe_log_api::Level::Error => {
                tracing::error!(target: "airframe_log_api", message = %args)
            }
            airframe_log_api::Level::Warn => {
                tracing::warn!( target: "airframe_log_api", message = %args)
            }
            airframe_log_api::Level::Info => {
                tracing::info!( target: "airframe_log_api", message = %args)
            }
            airframe_log_api::Level::Debug => {
                tracing::debug!(target: "airframe_log_api", message = %args)
            }
            airframe_log_api::Level::Trace => {
                tracing::trace!(target: "airframe_log_api", message = %args)
            }
        }
    }

    fn enabled(&self, _level: airframe_log_api::Level) -> bool {
        // Defer to tracing for filtering; returning true means we always forward
        // and let tracing's configured filters/sinks decide.
        true
    }
}

/// Install the bridge logger for airframe_log_api. Safe to call multiple times; only the first wins.
pub fn install_airframe_log_api_bridge() {
    let _ = airframe_log_api::set_logger(&TRACING_LOGGER);
}
