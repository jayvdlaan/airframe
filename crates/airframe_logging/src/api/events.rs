//! Logging runtime events.

#[cfg(feature = "module")]
use airframe_core::bus::Event;

use crate::api::config::{LoggingConfig, SinkConfig};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoggingChanged;
#[cfg(feature = "module")]
impl Event for LoggingChanged {
    const NAME: &'static str = "LoggingChanged";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetLogFilter {
    pub filter: String,
}
#[cfg(feature = "module")]
impl Event for SetLogFilter {
    const NAME: &'static str = "SetLogFilter";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetLogLevel {
    pub target: Option<String>,
    pub level: String,
}
#[cfg(feature = "module")]
impl Event for SetLogLevel {
    const NAME: &'static str = "SetLogLevel";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToggleJson {
    pub enabled: bool,
}
#[cfg(feature = "module")]
impl Event for ToggleJson {
    const NAME: &'static str = "ToggleJson";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetAnsi {
    pub enabled: bool,
}
#[cfg(feature = "module")]
impl Event for SetAnsi {
    const NAME: &'static str = "SetAnsi";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddSink {
    pub sink: SinkConfig,
}
#[cfg(feature = "module")]
impl Event for AddSink {
    const NAME: &'static str = "AddSink";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveSink {
    pub sink_id: usize,
}
#[cfg(feature = "module")]
impl Event for RemoveSink {
    const NAME: &'static str = "RemoveSink";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetSinkFilter {
    pub sink_id: usize,
    pub filter: Option<String>,
}
#[cfg(feature = "module")]
impl Event for SetSinkFilter {
    const NAME: &'static str = "SetSinkFilter";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetSinkFormat {
    pub sink_id: usize,
    pub json: Option<bool>,
    pub ansi: Option<bool>,
    pub with_span_events: Option<String>,
    pub include_correlation_id: Option<bool>,
}
#[cfg(feature = "module")]
impl Event for SetSinkFormat {
    const NAME: &'static str = "SetSinkFormat";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequestLoggingStatus;
#[cfg(feature = "module")]
impl Event for RequestLoggingStatus {
    const NAME: &'static str = "RequestLoggingStatus";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SinkDiag {
    pub sink_id: usize,
    pub kind: String,
    pub path: Option<String>,
    pub rotation: Option<String>,
    pub filter: Option<String>,
    pub json: Option<bool>,
    pub ansi: Option<bool>,
    pub with_span_events: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LoggingStatus {
    pub config: LoggingConfig,
    pub global_filter: Option<String>,
    pub sinks: Vec<SinkDiag>,
}
#[cfg(feature = "module")]
impl Event for LoggingStatus {
    const NAME: &'static str = "LoggingStatus";
}
