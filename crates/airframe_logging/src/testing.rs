use tracing_subscriber::prelude::*;

// Thread-local buffer to capture logs deterministically per test thread
thread_local! {
    static LOG_BUFFER: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

#[derive(Clone, Default)]
struct StringWriter;
impl std::io::Write for StringWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        LOG_BUFFER.with(|b| b.borrow_mut().push_str(&s));
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct Guard(Option<tracing::dispatcher::DefaultGuard>);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.take();
    }
}

/// Initialize a deterministic buffer sink and EnvFilter for tests.
/// - No timestamps, no ANSI.
/// - Includes level and target; message is included by default.
///   Returns a Guard that keeps the subscriber active for its lifetime.
pub fn init_for_test(filter: &str, json: bool) -> Guard {
    // Clear thread-local buffer at init to avoid leakage between tests on the same thread
    LOG_BUFFER.with(|b| b.borrow_mut().clear());
    let env = tracing_subscriber::EnvFilter::new(filter);
    if json {
        let make_writer = || StringWriter;
        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .with_target(true)
            .with_level(true)
            .without_time()
            .with_writer(make_writer);
        let subscriber = tracing_subscriber::registry::Registry::default()
            .with(env)
            .with(fmt_layer);
        let guard = tracing::subscriber::set_default(subscriber);
        Guard(Some(guard))
    } else {
        let make_writer = || StringWriter;
        let fmt_cfg = tracing_subscriber::fmt::format()
            .without_time()
            .with_target(true)
            .with_level(true);
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .event_format(fmt_cfg)
            .with_writer(make_writer);
        let subscriber = tracing_subscriber::registry::Registry::default()
            .with(env)
            .with(fmt_layer);
        let guard = tracing::subscriber::set_default(subscriber);
        Guard(Some(guard))
    }
}

/// Take and clear the current thread-local buffer content.
pub fn take() -> String {
    LOG_BUFFER.with(|b| {
        let mut s = b.borrow_mut();
        let out = s.clone();
        s.clear();
        out
    })
}
