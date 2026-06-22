//! Job lifecycle events published via the [`EventBus`](airframe_core::bus::EventBus).

use airframe_core::bus::Event;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JobStarted {
    pub id: String,
}
impl Event for JobStarted {
    const NAME: &'static str = "JobStarted";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JobCompleted {
    pub id: String,
}
impl Event for JobCompleted {
    const NAME: &'static str = "JobCompleted";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JobFailed {
    pub id: String,
    pub error: String,
}
impl Event for JobFailed {
    const NAME: &'static str = "JobFailed";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JobRetry {
    pub id: String,
    pub attempt: u32,
}
impl Event for JobRetry {
    const NAME: &'static str = "JobRetry";
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct JobSkipped {
    pub id: String,
    pub reason: String,
}
impl Event for JobSkipped {
    const NAME: &'static str = "JobSkipped";
}
