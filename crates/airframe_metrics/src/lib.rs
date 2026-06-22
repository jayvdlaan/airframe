//! airframe_metrics — generic metrics primitives with Prometheus exposition.
//!
//! Provides `Counter`, `Gauge`, `DurationMetric`, and a `MetricsRegistry` that
//! renders Prometheus text format. Also provides a `MetricsModule` that registers
//! the registry in the `ServiceRegistry` and an optional `/metrics` HTTP endpoint
//! (behind the `http` feature flag).

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use airframe_core::module::{CAP_METRICS, Module, ModuleContext, ModuleDescriptor};
use airframe_core::registry::ServiceRegistry;
use airframe_macros::module_descriptor;
use async_trait::async_trait;
use tracing::info;

#[cfg(feature = "otel")]
pub mod otel;

// ---------------------------------------------------------------------------
// Core metric types
// ---------------------------------------------------------------------------

/// Monotonically increasing counter with optional labels.
pub struct Counter {
    name: String,
    help: String,
    unlabeled: AtomicU64,
    labeled: Mutex<HashMap<Vec<(String, String)>, u64>>,
}

impl Counter {
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            unlabeled: AtomicU64::new(0),
            labeled: Mutex::new(HashMap::new()),
        }
    }

    /// Increment the unlabeled counter by 1.
    pub fn inc(&self) {
        self.unlabeled.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the unlabeled counter by `n`.
    pub fn inc_by(&self, n: u64) {
        self.unlabeled.fetch_add(n, Ordering::Relaxed);
    }

    /// Increment a labeled counter by 1.
    pub fn inc_labeled(&self, labels: &[(&str, &str)]) {
        let key: Vec<(String, String)> = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut g = self.labeled.lock().unwrap();
        *g.entry(key).or_insert(0) += 1;
    }

    /// Get the current unlabeled value.
    pub fn get(&self) -> u64 {
        self.unlabeled.load(Ordering::Relaxed)
    }

    pub fn render(&self, out: &mut String) {
        if !self.help.is_empty() {
            out.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        }
        out.push_str(&format!("# TYPE {} counter\n", self.name));

        let labeled = self.labeled.lock().unwrap();
        if labeled.is_empty() {
            out.push_str(&format!(
                "{} {}\n",
                self.name,
                self.unlabeled.load(Ordering::Relaxed)
            ));
        } else {
            let unlabeled_val = self.unlabeled.load(Ordering::Relaxed);
            if unlabeled_val > 0 {
                out.push_str(&format!("{} {}\n", self.name, unlabeled_val));
            }
            for (labels, count) in labeled.iter() {
                out.push_str(&self.name);
                out.push('{');
                for (i, (k, v)) in labels.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&format!("{}=\"{}\"", k, escape_label_value(v)));
                }
                out.push_str("} ");
                out.push_str(&count.to_string());
                out.push('\n');
            }
        }
    }
}

/// Gauge that can go up and down.
pub struct Gauge {
    name: String,
    help: String,
    value: AtomicI64,
}

impl Gauge {
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            value: AtomicI64::new(0),
        }
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn set(&self, val: i64) {
        self.value.store(val, Ordering::Relaxed);
    }

    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }

    pub fn render(&self, out: &mut String) {
        if !self.help.is_empty() {
            out.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        }
        out.push_str(&format!("# TYPE {} gauge\n", self.name));
        out.push_str(&format!(
            "{} {}\n",
            self.name,
            self.value.load(Ordering::Relaxed)
        ));
    }
}

/// Duration accumulator (sum + count) for summary-style metrics.
///
/// Internally stores microseconds for sub-millisecond precision.
/// `render()` outputs `_sum` in seconds.
pub struct DurationMetric {
    name: String,
    help: String,
    sum_us: AtomicU64,
    count: AtomicU64,
}

impl DurationMetric {
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            sum_us: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    /// Record an observation in milliseconds.
    pub fn observe_ms(&self, ms: u64) {
        self.sum_us.fetch_add(ms * 1000, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an observation in microseconds (for sub-ms precision).
    pub fn observe_us(&self, us: u64) {
        self.sum_us.fetch_add(us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn sum_ms(&self) -> u64 {
        self.sum_us.load(Ordering::Relaxed) / 1000
    }

    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    pub fn render(&self, out: &mut String) {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return;
        }
        if !self.help.is_empty() {
            out.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        }
        out.push_str(&format!("# TYPE {} summary\n", self.name));
        let sum_sec = (self.sum_us.load(Ordering::Relaxed) as f64) / 1_000_000.0;
        out.push_str(&format!("{}_sum {}\n", self.name, sum_sec));
        out.push_str(&format!("{}_count {}\n", self.name, count));
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RegistryInner {
    counters: Vec<Arc<Counter>>,
    gauges: Vec<Arc<Gauge>>,
    durations: Vec<Arc<DurationMetric>>,
}

/// Thread-safe metrics registry. Cloning shares the underlying state.
#[derive(Clone, Default)]
pub struct MetricsRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or retrieve a counter with the given name (no help text).
    pub fn counter(&self, name: &str) -> Arc<Counter> {
        self.counter_with_help(name, "")
    }

    /// Register or retrieve a counter with help text.
    pub fn counter_with_help(&self, name: &str, help: &str) -> Arc<Counter> {
        let mut g = self.inner.lock().unwrap();
        if let Some(existing) = g.counters.iter().find(|c| c.name == name) {
            return existing.clone();
        }
        let c = Arc::new(Counter::new(name, help));
        g.counters.push(c.clone());
        c
    }

    /// Register or retrieve a gauge with the given name (no help text).
    pub fn gauge(&self, name: &str) -> Arc<Gauge> {
        self.gauge_with_help(name, "")
    }

    /// Register or retrieve a gauge with help text.
    pub fn gauge_with_help(&self, name: &str, help: &str) -> Arc<Gauge> {
        let mut g = self.inner.lock().unwrap();
        if let Some(existing) = g.gauges.iter().find(|c| c.name == name) {
            return existing.clone();
        }
        let gauge = Arc::new(Gauge::new(name, help));
        g.gauges.push(gauge.clone());
        gauge
    }

    /// Register or retrieve a duration metric with the given name (no help text).
    pub fn duration(&self, name: &str) -> Arc<DurationMetric> {
        self.duration_with_help(name, "")
    }

    /// Register or retrieve a duration metric with help text.
    pub fn duration_with_help(&self, name: &str, help: &str) -> Arc<DurationMetric> {
        let mut g = self.inner.lock().unwrap();
        if let Some(existing) = g.durations.iter().find(|d| d.name == name) {
            return existing.clone();
        }
        let d = Arc::new(DurationMetric::new(name, help));
        g.durations.push(d.clone());
        d
    }

    /// Render all registered metrics in Prometheus exposition text format.
    pub fn render(&self) -> String {
        let g = self.inner.lock().unwrap();
        let mut out = String::new();
        for c in &g.counters {
            c.render(&mut out);
        }
        for gauge in &g.gauges {
            gauge.render(&mut out);
        }
        for d in &g.durations {
            d.render(&mut out);
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Snapshot API
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of a single metric for export.
#[derive(Debug, Clone)]
pub enum MetricSnapshot {
    Counter {
        name: String,
        help: String,
        value: u64,
    },
    Gauge {
        name: String,
        help: String,
        value: i64,
    },
    Duration {
        name: String,
        help: String,
        sum_ms: u64,
        count: u64,
    },
}

impl MetricsRegistry {
    /// Snapshot all registered metrics for export.
    ///
    /// Reads atomic values and returns owned data — no lock held during
    /// subsequent processing.
    pub fn snapshot(&self) -> Vec<MetricSnapshot> {
        let g = self.inner.lock().unwrap();
        let mut out = Vec::with_capacity(g.counters.len() + g.gauges.len() + g.durations.len());
        for c in &g.counters {
            out.push(MetricSnapshot::Counter {
                name: c.name.clone(),
                help: c.help.clone(),
                value: c.get(),
            });
        }
        for gauge in &g.gauges {
            out.push(MetricSnapshot::Gauge {
                name: gauge.name.clone(),
                help: gauge.help.clone(),
                value: gauge.get(),
            });
        }
        for d in &g.durations {
            out.push(MetricSnapshot::Duration {
                name: d.name.clone(),
                help: d.help.clone(),
                sum_ms: d.sum_ms(),
                count: d.count(),
            });
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Escape a label value for Prometheus exposition format.
/// Escapes backslash, double-quote, and newline characters.
pub fn escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

// ---------------------------------------------------------------------------
// ServiceRegistry extension trait
// ---------------------------------------------------------------------------

/// Extension trait for convenient access to `MetricsRegistry` from the
/// `ServiceRegistry`.
pub trait ServiceRegistryMetricsExt {
    fn metrics(&self) -> Option<Arc<MetricsRegistry>>;
}

impl ServiceRegistryMetricsExt for ServiceRegistry {
    fn metrics(&self) -> Option<Arc<MetricsRegistry>> {
        self.get::<MetricsRegistry>()
    }
}

// ---------------------------------------------------------------------------
// MetricsModule
// ---------------------------------------------------------------------------

/// Airframe module that registers a `MetricsRegistry` in the `ServiceRegistry`.
pub struct MetricsModule {
    desc: ModuleDescriptor,
}

impl Default for MetricsModule {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "metrics",
                version: "0.1.0",
                provides: [CAP_METRICS.0]
            ),
        }
    }
}

#[async_trait]
impl Module for MetricsModule {
    airframe_macros::impl_descriptor!();

    async fn init(&mut self, ctx: ModuleContext) -> anyhow::Result<()> {
        info!(target = "airframe_metrics", "metrics module initialized");
        let registry = Arc::new(MetricsRegistry::new());
        ctx.services.register::<MetricsRegistry>(registry);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_inc_and_get() {
        let c = Counter::new("test_counter", "");
        assert_eq!(c.get(), 0);
        c.inc();
        c.inc();
        assert_eq!(c.get(), 2);
    }

    #[test]
    fn counter_inc_by() {
        let c = Counter::new("test_counter", "");
        c.inc_by(5);
        assert_eq!(c.get(), 5);
    }

    #[test]
    fn counter_labeled() {
        let c = Counter::new("test_counter", "");
        c.inc_labeled(&[("method", "GET")]);
        c.inc_labeled(&[("method", "GET")]);
        c.inc_labeled(&[("method", "POST")]);
        let mut out = String::new();
        c.render(&mut out);
        assert!(out.contains(r#"test_counter{method="GET"} 2"#));
        assert!(out.contains(r#"test_counter{method="POST"} 1"#));
    }

    #[test]
    fn gauge_inc_dec_set() {
        let g = Gauge::new("test_gauge", "");
        assert_eq!(g.get(), 0);
        g.inc();
        g.inc();
        g.dec();
        assert_eq!(g.get(), 1);
        g.set(42);
        assert_eq!(g.get(), 42);
    }

    #[test]
    fn duration_metric_observe() {
        let d = DurationMetric::new("test_duration", "");
        d.observe_ms(1500);
        d.observe_ms(500);
        assert_eq!(d.sum_ms(), 2000);
        assert_eq!(d.count(), 2);
    }

    #[test]
    fn duration_metric_no_render_when_empty() {
        let d = DurationMetric::new("test_duration", "");
        let mut out = String::new();
        d.render(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn duration_metric_render_seconds() {
        let d = DurationMetric::new("op_duration", "Operation duration");
        d.observe_ms(2000);
        let mut out = String::new();
        d.render(&mut out);
        assert!(out.contains("op_duration_sum 2"));
        assert!(out.contains("op_duration_count 1"));
    }

    #[test]
    fn registry_counter_dedup() {
        let reg = MetricsRegistry::new();
        let c1 = reg.counter("test");
        let c2 = reg.counter("test");
        assert!(Arc::ptr_eq(&c1, &c2));
    }

    #[test]
    fn registry_gauge_dedup() {
        let reg = MetricsRegistry::new();
        let g1 = reg.gauge("test");
        let g2 = reg.gauge("test");
        assert!(Arc::ptr_eq(&g1, &g2));
    }

    #[test]
    fn registry_duration_dedup() {
        let reg = MetricsRegistry::new();
        let d1 = reg.duration("test");
        let d2 = reg.duration("test");
        assert!(Arc::ptr_eq(&d1, &d2));
    }

    #[test]
    fn registry_render_includes_all_types() {
        let reg = MetricsRegistry::new();
        let c = reg.counter_with_help("http_total", "Total requests");
        c.inc();
        let g = reg.gauge_with_help("active_conns", "Active connections");
        g.set(5);
        let d = reg.duration_with_help("req_duration", "Request duration");
        d.observe_ms(100);

        let out = reg.render();
        assert!(out.contains("# HELP http_total Total requests"));
        assert!(out.contains("# TYPE http_total counter"));
        assert!(out.contains("http_total 1"));
        assert!(out.contains("# TYPE active_conns gauge"));
        assert!(out.contains("active_conns 5"));
        assert!(out.contains("req_duration_sum 0.1"));
        assert!(out.contains("req_duration_count 1"));
    }

    #[test]
    fn registry_clone_shares_state() {
        let reg = MetricsRegistry::new();
        let reg2 = reg.clone();
        let c = reg.counter("shared");
        c.inc();
        let c2 = reg2.counter("shared");
        assert_eq!(c2.get(), 1);
    }

    #[test]
    fn escape_label_value_handles_special_chars() {
        assert_eq!(escape_label_value("clean"), "clean");
        assert_eq!(escape_label_value(r#"has"quote"#), r#"has\"quote"#);
        assert_eq!(escape_label_value(r"has\backslash"), r"has\\backslash");
        assert_eq!(escape_label_value("has\nnewline"), r"has\nnewline");
        assert_eq!(escape_label_value(r#"both\"mixed"#), r#"both\\\"mixed"#);
    }

    #[test]
    fn snapshot_captures_all_types() {
        let reg = MetricsRegistry::new();
        let c = reg.counter_with_help("snap_counter", "A counter");
        c.inc();
        c.inc();
        let g = reg.gauge_with_help("snap_gauge", "A gauge");
        g.set(42);
        let d = reg.duration_with_help("snap_duration", "A duration");
        d.observe_ms(100);
        d.observe_ms(200);

        let snaps = reg.snapshot();
        assert_eq!(snaps.len(), 3);

        match &snaps[0] {
            MetricSnapshot::Counter { name, value, .. } => {
                assert_eq!(name, "snap_counter");
                assert_eq!(*value, 2);
            }
            _ => panic!("expected Counter"),
        }
        match &snaps[1] {
            MetricSnapshot::Gauge { name, value, .. } => {
                assert_eq!(name, "snap_gauge");
                assert_eq!(*value, 42);
            }
            _ => panic!("expected Gauge"),
        }
        match &snaps[2] {
            MetricSnapshot::Duration {
                name,
                sum_ms,
                count,
                ..
            } => {
                assert_eq!(name, "snap_duration");
                assert_eq!(*sum_ms, 300);
                assert_eq!(*count, 2);
            }
            _ => panic!("expected Duration"),
        }
    }

    #[test]
    fn service_registry_ext() {
        let sr = ServiceRegistry::default();
        assert!(sr.metrics().is_none());
        sr.register::<MetricsRegistry>(Arc::new(MetricsRegistry::new()));
        assert!(sr.metrics().is_some());
    }
}
