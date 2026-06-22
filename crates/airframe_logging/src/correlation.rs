// Correlation ID task-local storage for async contexts
// Allows setting/getting a correlation_id per-task, usable by logging/formatting layers.
tokio::task_local! {
    static CORRELATION_ID: std::cell::RefCell<Option<String>>;
}

/// Get the current correlation_id if set in this task.
pub fn get() -> Option<String> {
    CORRELATION_ID
        .try_with(|c| c.borrow().clone())
        .ok()
        .flatten()
}
/// Set/override the correlation_id for the current task if the task-local is initialized.
pub fn set(id: impl Into<String>) {
    let _ = CORRELATION_ID.try_with(|c| *c.borrow_mut() = Some(id.into()));
}
/// Clear the correlation_id for the current task if the task-local is initialized.
pub fn clear() {
    let _ = CORRELATION_ID.try_with(|c| *c.borrow_mut() = None);
}
/// Run the given future within a scope that has the provided correlation_id set for this async task.
/// Example: correlation::scope("req-123", async { ... }).await;
pub async fn scope<F, T>(id: impl Into<String>, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let id = id.into();
    // Enter a span carrying correlation_id so formatters that include span fields can display it
    let span = tracing::span!(tracing::Level::INFO, "corr", correlation_id = %id);
    let _enter = span.enter();
    let cell = std::cell::RefCell::new(Some(id));
    CORRELATION_ID.scope(cell, fut).await
}
