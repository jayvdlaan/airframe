use std::time::Duration;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

// Marker traits for message types
pub trait Event: Serialize + DeserializeOwned + Send + Sync + 'static {
    const NAME: &'static str;
}
pub trait Command: Serialize + DeserializeOwned + Send + Sync + 'static {
    const NAME: &'static str;
}
pub trait Query: Serialize + DeserializeOwned + Send + Sync + 'static {
    const NAME: &'static str;
}

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish<E: Event>(&self, _evt: E, _timeout: Option<Duration>) -> anyhow::Result<()>;
    fn subscribe<E: Event>(&self) -> anyhow::Result<ReceiverStream<E>>;
}

#[async_trait]
pub trait CommandBus: Send + Sync {
    async fn dispatch<C: Command>(&self, _cmd: C, _timeout: Option<Duration>)
        -> anyhow::Result<()>;

    fn register_handler<C, F>(&self, _handler: F) -> anyhow::Result<()>
    where
        C: Command,
        F: Fn(C, CancellationToken) -> futures::future::BoxFuture<'static, anyhow::Result<()>>
            + Send
            + Sync
            + 'static;
}

#[async_trait]
pub trait QueryBus: Send + Sync {
    async fn ask<Q: Query, R: Serialize + DeserializeOwned + Send + Sync + 'static>(
        &self,
        _q: Q,
        _timeout: Option<Duration>,
    ) -> anyhow::Result<R>;

    fn register_handler<Q, R, F>(&self, _handler: F) -> anyhow::Result<()>
    where
        Q: Query,
        R: Serialize + DeserializeOwned + Send + Sync + 'static,
        F: Fn(Q, CancellationToken) -> futures::future::BoxFuture<'static, anyhow::Result<R>>
            + Send
            + Sync
            + 'static;
}

/// Spawn a background task that invokes `handler` for each event of type `E`
/// published on `bus`, stopping cleanly when `cancel` is triggered or the
/// subscription ends.
///
/// This consolidates the subscribe → spawn → next-loop → cancellation-check
/// scaffold that adapter modules (logging, kv, config, …) previously hand-rolled.
pub fn spawn_event_watcher<B, E, F, Fut>(
    bus: &B,
    cancel: CancellationToken,
    mut handler: F,
) -> anyhow::Result<()>
where
    B: EventBus + ?Sized,
    E: Event,
    F: FnMut(E) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let mut stream = bus.subscribe::<E>()?;
    tokio::spawn(async move {
        use tokio_stream::StreamExt;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                next = stream.next() => match next {
                    Some(evt) => handler(evt).await,
                    None => break,
                },
            }
        }
    });
    Ok(())
}

// In-memory implementations live under this nested module to keep core API slim
pub mod inmem;

// Zero-serialization typed event bus (no Serialize/DeserializeOwned required)
pub mod typed;
