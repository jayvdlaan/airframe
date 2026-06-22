//! Run with:
//!   cargo run -p airframe_core --example spacetime_health_example --features airframe-spacetime

#[cfg(feature = "airframe-spacetime")]
mod real_main {
    use spacetime_core::{Duration, Instant};
    use spacetime_health::HealthSnapshot;
    pub fn run() {
        let snap =
            HealthSnapshot::healthy(Instant::from_millis_since_epoch(1), Duration::from_secs(2));
        println!(
            "health: {:?}, since={}, uptime_ms={}",
            snap.status,
            snap.since.millis_since_epoch,
            snap.uptime.as_millis()
        );
    }
}

fn main() {
    #[cfg(feature = "airframe-spacetime")]
    real_main::run();
    #[cfg(not(feature = "airframe-spacetime"))]
    {
        // Feature not enabled; this example is a no-op to allow the crate to build in default CI.
        println!("spacetime_health_example: enable --features airframe-spacetime to run");
    }
}
