//! Internal helper module exposing the ParentSubscriber alias used by layers.
//! This mirrors the alias defined in lib.rs to allow submodules to reference it without circular deps.

pub type ParentSubscriber = tracing_subscriber::layer::Layered<
    tracing_subscriber::reload::Layer<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>,
    tracing_subscriber::Registry,
>;
