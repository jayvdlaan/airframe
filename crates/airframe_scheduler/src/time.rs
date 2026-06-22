//! Internal time helpers shared by the scheduler.

use std::sync::Arc;
use std::time::Duration;

use spacetime_core as st;

pub(crate) fn now_ms() -> u64 {
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as u64
}

pub(crate) async fn sleep_with(_rt: Option<Arc<dyn st::Runtime + Send + Sync>>, d: Duration) {
    #[cfg(feature = "airframe-spacetime")]
    if let Some(rt) = _rt {
        let st_dur = st::Duration::from_millis(d.as_millis() as u64);
        // spacetime Timer::sleep is blocking; move to blocking thread.
        let _ = tokio::task::spawn_blocking(move || rt.timer().sleep(st_dur)).await;
        return;
    }
    // Fallback to Tokio sleep
    tokio::time::sleep(d).await
}
