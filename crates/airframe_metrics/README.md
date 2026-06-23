# airframe_metrics

Metrics primitives (`Counter`, `Gauge`, `DurationMetric`) with a registry that renders the Prometheus exposition text format for Airframe.

## Overview

`airframe_metrics` provides lightweight, thread-safe metric types and a shared
registry:

- `Counter` — monotonically increasing value with `inc`, `inc_by`, and
  labeled increments (`inc_labeled`).
- `Gauge` — value that can go up and down (`inc`, `dec`, `set`, `get`).
- `DurationMetric` — sum/count accumulator for summary-style timings
  (`observe_ms`, `observe_us`), stored internally in microseconds and rendered
  as `_sum` (seconds) and `_count`.
- `MetricsRegistry` — thread-safe, cheaply cloneable registry that
  deduplicates metrics by name and renders them all via `render()` (Prometheus
  text format) or captures them with `snapshot()` (a `Vec<MetricSnapshot>` of
  owned values).
- `escape_label_value` — escapes backslash, double-quote, and newline
  characters for Prometheus label values.
- `ServiceRegistryMetricsExt` — extension trait adding a `metrics()` accessor
  to `ServiceRegistry`.

An optional OTLP push exporter (`otel` feature) bridges a `MetricsRegistry`
into the OpenTelemetry SDK pipeline.

## Airframe module compatibility

Provides `MetricsModule`, which implements the airframe `Module` trait. On
`init` it registers a fresh `MetricsRegistry` in the `ServiceRegistry`.

- `provides`: `cap:metrics`
- `requires`: none

Other modules retrieve the registry via `ServiceRegistry::get::<MetricsRegistry>()`
or the `ServiceRegistryMetricsExt::metrics()` convenience method.

Note: `MetricsModule` only registers the registry — it does not mount any HTTP
route or expose a scrape endpoint (see Status).

## Dependencies

- `airframe_core` — module system and `ServiceRegistry`
- `airframe_macros` — `module_descriptor!` macro
- `airframe_http` (optional, behind the `http` feature) — not currently used by
  any code in this crate (see Status)
- `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` (optional, behind
  the `otel` feature) — OTLP push exporter

## Usage

```rust
use airframe_metrics::{MetricsRegistry, MetricSnapshot};

let registry = MetricsRegistry::new();

// Register (or retrieve, by name) metrics. The registry hands back Arc handles.
let requests = registry.counter_with_help("http_requests_total", "Total HTTP requests");
let in_flight = registry.gauge_with_help("http_in_flight", "Requests in flight");
let latency = registry.duration_with_help("http_request_duration", "Request duration");

// Record some activity.
requests.inc();
requests.inc_labeled(&[("method", "GET")]);
in_flight.set(3);
latency.observe_ms(120);

// Render Prometheus text exposition format.
let text = registry.render();
assert!(text.contains("# TYPE http_requests_total counter"));

// Or take an owned, lock-free snapshot for custom export.
for snap in registry.snapshot() {
    if let MetricSnapshot::Counter { name, value, .. } = snap {
        println!("{name} = {value}");
    }
}
```
