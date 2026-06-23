# airframe_log_api

Lightweight logging API for Airframe: Logger trait, global setter, and zero-cost macros when no logger is installed.

## Overview

This crate provides a minimal logging facade that libraries can use without pulling in heavy dependencies. When no logger is installed, all logging macros are true no-ops that don't evaluate their arguments.

**Key properties:**
- Zero cost when unused (macros short-circuit before argument evaluation)
- Single global logger installation (set once at startup)
- No dependencies beyond `std`
- Compatible with any logging backend

## Usage

### For Libraries

Libraries should use the logging macros without worrying about whether a logger is installed:

```rust
use airframe_log_api::{error, warn, info, debug, trace};

pub fn do_something() {
    info!("Starting operation");

    if let Err(e) = risky_operation() {
        error!("Operation failed: {}", e);
    }

    debug!("Debug details: {:?}", some_value);
}
```

If no logger is installed, these calls do nothing and don't evaluate the format arguments.

### For Applications

Applications install a logger once at startup:

```rust
use airframe_log_api::{Logger, Level, set_logger};
use std::fmt;

struct StderrLogger;

impl Logger for StderrLogger {
    fn log(&self, level: Level, args: &fmt::Arguments) {
        eprintln!("[{:?}] {}", level, args);
    }

    fn enabled(&self, level: Level) -> bool {
        level <= Level::Info  // Filter out Debug and Trace
    }
}

static LOGGER: StderrLogger = StderrLogger;

fn main() {
    set_logger(&LOGGER).expect("logger already set");

    // Now all logging macros will output to stderr
    airframe_log_api::info!("Application started");
}
```

## API

### Types

- `Level` - Log levels: `Error`, `Warn`, `Info`, `Debug`, `Trace` (ordered by severity)
- `Logger` - Trait for log backends to implement
- `SetLoggerError` - Returned when attempting to set logger twice

### Functions

- `set_logger(logger)` - Install global logger (once only, returns `Result`)
- `is_enabled()` - Check if a logger has been installed
- `log(level, args)` - Low-level log function (prefer macros)

### Macros

All macros are no-ops when no logger is installed:

- `error!(...)` - Log at ERROR level
- `warn!(...)` - Log at WARN level
- `info!(...)` - Log at INFO level
- `debug!(...)` - Log at DEBUG level
- `trace!(...)` - Log at TRACE level

## Logger Trait

```rust
pub trait Logger: Sync + Send {
    /// Process a log message. Should be fast and non-panicking.
    fn log(&self, level: Level, args: &fmt::Arguments);

    /// Check if level is enabled. Default: all levels enabled.
    fn enabled(&self, level: Level) -> bool { true }
}
```

The `enabled` method allows loggers to filter messages before formatting, providing an additional optimization point.

## Integration with airframe_logging

This crate provides the low-level API. For full-featured logging with:
- Multiple sinks (console, file, syslog, journald)
- Structured logging with tracing integration
- Hot-reload configuration
- Correlation IDs

Use `airframe_logging` which builds on this API.

## Design Rationale

**Why not use `log` or `tracing` directly?**

This crate provides a minimal facade that:
1. Has zero dependencies (important for low-level crates)
2. Guarantees zero-cost when unused (argument short-circuiting)
3. Allows Airframe to control the logging abstraction layer

Libraries in the Airframe ecosystem can depend on this lightweight crate, while applications choose their preferred backend.

## License

Licensed under the MIT License.
