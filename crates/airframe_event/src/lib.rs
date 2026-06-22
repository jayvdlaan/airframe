//! airframe_event: shared event contracts/helpers layered on airframe_core buses.
//!
//! This crate currently re-exports the core Event trait and provides a minimal
//! common Tick event used in examples/tests.

pub use airframe_core::bus::{Event, EventBus};

/// A simple counter event used in examples and tests.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Tick(pub u64);
impl Event for Tick {
    const NAME: &'static str = "Tick";
}

/// Crate identity string.
pub const CRATE: &str = "airframe_event";

/// Simple readiness check placeholder (kept consistent across crates).
pub fn ping() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_event_name_and_derives() {
        let t = Tick(42);
        assert_eq!(Tick::NAME, "Tick");
        let json = serde_json::to_string(&t).unwrap();
        let back: Tick = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn crate_const_and_ping() {
        assert_eq!(CRATE, "airframe_event");
        assert!(ping());
    }
}
