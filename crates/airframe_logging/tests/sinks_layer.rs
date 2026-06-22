//! Tests for the sinks layer filter logic.

use airframe_logging::testing;

#[test]
fn sinks_layer_filters_by_target() {
    // Initialize with a filter that only allows my_target at info
    let _guard = testing::init_for_test("my_target=info", false);

    // Log at info level to my_target - should appear
    tracing::info!(target: "my_target", "should appear");
    let out = testing::take();
    assert!(
        out.contains("should appear"),
        "info to my_target should be logged: {}",
        out
    );

    // Log at debug level to my_target - should not appear
    tracing::debug!(target: "my_target", "should not appear");
    let out = testing::take();
    assert!(
        !out.contains("should not appear"),
        "debug to my_target should be filtered: {}",
        out
    );
}

#[test]
fn sinks_layer_filters_by_level() {
    let _guard = testing::init_for_test("warn", false);

    // Warn should appear
    tracing::warn!(target: "test", "warning message");
    let out = testing::take();
    assert!(
        out.contains("warning message"),
        "warn should appear: {}",
        out
    );

    // Info should be filtered
    tracing::info!(target: "test", "info message");
    let out = testing::take();
    assert!(
        !out.contains("info message"),
        "info should be filtered: {}",
        out
    );
}

#[test]
fn sinks_layer_json_format() {
    let _guard = testing::init_for_test("info", true);

    tracing::info!(target: "json_test", msg = "structured");
    let out = testing::take();

    // JSON output should contain level field
    assert!(
        out.contains("\"level\""),
        "should have level field: {}",
        out
    );
    assert!(out.contains("structured"), "should have message: {}", out);
}

#[test]
fn sinks_layer_multiple_directives() {
    let _guard = testing::init_for_test("warn,my_crate=debug,other=error", false);

    // my_crate at debug should appear
    tracing::debug!(target: "my_crate::sub", "my_crate debug");
    let out = testing::take();
    assert!(
        out.contains("my_crate debug"),
        "my_crate debug should appear: {}",
        out
    );

    // other at warn should be filtered (needs error)
    tracing::warn!(target: "other", "other warn");
    let out = testing::take();
    assert!(
        !out.contains("other warn"),
        "other warn should be filtered: {}",
        out
    );

    // other at error should appear
    tracing::error!(target: "other", "other error");
    let out = testing::take();
    assert!(
        out.contains("other error"),
        "other error should appear: {}",
        out
    );

    // random target at warn should appear (default)
    tracing::warn!(target: "random", "random warn");
    let out = testing::take();
    assert!(
        out.contains("random warn"),
        "random warn should appear: {}",
        out
    );
}

#[test]
fn sinks_layer_span_events() {
    let _guard = testing::init_for_test("info", false);

    let span = tracing::info_span!("my_span");
    let _enter = span.enter();
    tracing::info!("inside span");
    let out = testing::take();
    assert!(out.contains("inside span"), "event inside span: {}", out);
}

#[test]
fn sinks_layer_with_fields() {
    let _guard = testing::init_for_test("info", false);

    tracing::info!(target: "field_test", foo = "bar", count = 42, "event with fields");
    let out = testing::take();
    assert!(
        out.contains("event with fields"),
        "should have message: {}",
        out
    );
    assert!(
        out.contains("foo") || out.contains("bar"),
        "should have field: {}",
        out
    );
}

#[test]
fn sinks_layer_empty_filter() {
    // Empty filter should allow nothing (or use defaults)
    let _guard = testing::init_for_test("", false);

    // This may or may not appear depending on default behavior
    tracing::info!(target: "empty_filter", "test");
    let _out = testing::take();
    // Just verify no panic
}

#[test]
fn sinks_layer_trace_level() {
    let _guard = testing::init_for_test("trace", false);

    tracing::trace!(target: "trace_test", "trace message");
    let out = testing::take();
    assert!(
        out.contains("trace message"),
        "trace should appear: {}",
        out
    );
}
