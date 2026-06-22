//! Convenient re-exports for common types when using airframe_logging.
//! Import with: `use airframe_logging::prelude::*;`

pub use crate::api::config::{FormatOptions, LoggingConfig, RotationConfig, SinkConfig};
pub use crate::api::events::{
    AddSink, LoggingChanged, LoggingStatus, RemoveSink, RequestLoggingStatus, SetAnsi,
    SetLogFilter, SetLogLevel, SetSinkFilter, SetSinkFormat, SinkDiag, ToggleJson,
};
pub use crate::runtime::state::{LoggingControl, LoggingState};

#[cfg(feature = "module")]
pub use crate::module::LoggingModule;
