//! OTLP push exporter for `MetricsRegistry`.
//!
//! Bridges airframe metrics into the OpenTelemetry SDK pipeline using
//! observable instruments. The SDK's `PeriodicReader` handles batching
//! and pushing via the OTLP protocol (gRPC/tonic by default).
//!
//! Gated behind the `otel` feature flag.

use std::time::Duration;

use opentelemetry::metrics::MeterProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use tracing::info;

use crate::{MetricSnapshot, MetricsRegistry};

/// OTLP metrics exporter that bridges `MetricsRegistry` into the
/// OpenTelemetry SDK pipeline.
///
/// On creation, snapshots all currently registered metrics and creates
/// corresponding OTel observable instruments. The SDK's `PeriodicReader`
/// periodically invokes the callbacks, which read the live atomic values
/// from the airframe metrics, and pushes them to the configured OTLP
/// endpoint.
///
/// Metrics registered *after* the exporter is created are not exported.
/// In practice this is fine because server metrics are all registered at
/// startup before the exporter is initialized.
pub struct OtelExporter {
    provider: SdkMeterProvider,
}

impl OtelExporter {
    /// Create and start the exporter.
    ///
    /// - `registry`: the shared metrics registry to bridge.
    /// - `endpoint`: OTLP gRPC endpoint (e.g. `http://localhost:4317`).
    /// - `interval`: how often to push (default recommendation: 15 s).
    pub fn new(
        registry: MetricsRegistry,
        endpoint: &str,
        interval: Duration,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        let reader = PeriodicReader::builder(exporter)
            .with_interval(interval)
            .build();

        let provider = SdkMeterProvider::builder().with_reader(reader).build();

        let meter = provider.meter("airframe_metrics");

        // Bridge each metric from our registry into an OTel observable
        // instrument.  The callbacks capture Arc<Counter/Gauge/Duration>
        // and read the live atomic values on every export cycle.
        for snap in registry.snapshot() {
            match snap {
                MetricSnapshot::Counter {
                    ref name, ref help, ..
                } => {
                    let counter = registry.counter(name);
                    meter
                        .u64_observable_counter(name.clone())
                        .with_description(help.clone())
                        .with_callback(move |observer| {
                            observer.observe(counter.get(), &[]);
                        })
                        .build();
                }
                MetricSnapshot::Gauge {
                    ref name, ref help, ..
                } => {
                    let gauge = registry.gauge(name);
                    meter
                        .i64_observable_gauge(name.clone())
                        .with_description(help.clone())
                        .with_callback(move |observer| {
                            observer.observe(gauge.get(), &[]);
                        })
                        .build();
                }
                MetricSnapshot::Duration {
                    ref name, ref help, ..
                } => {
                    let d_sum = registry.duration(name);
                    let d_cnt = registry.duration(name);
                    meter
                        .f64_observable_gauge(format!("{name}_seconds_sum"))
                        .with_description(format!("{help} (sum)"))
                        .with_callback(move |observer| {
                            observer.observe(d_sum.sum_ms() as f64 / 1000.0, &[]);
                        })
                        .build();
                    meter
                        .u64_observable_counter(format!("{name}_count"))
                        .with_description(format!("{help} (count)"))
                        .with_callback(move |observer| {
                            observer.observe(d_cnt.count(), &[]);
                        })
                        .build();
                }
            }
        }

        info!(
            endpoint,
            interval_secs = interval.as_secs(),
            "OTel metrics exporter started"
        );

        Ok(Self { provider })
    }

    /// Flush pending metrics and shut down the exporter.
    pub fn shutdown(&self) -> Result<(), opentelemetry_sdk::error::OTelSdkError> {
        self.provider.shutdown()
    }
}
