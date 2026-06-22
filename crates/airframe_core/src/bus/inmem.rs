//! Minimal placeholder in-memory bus implementations.
//! These are scaffolding for future fully-featured in-memory buses.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use std::{any::TypeId, sync::Arc};
use tokio::{
    sync::{broadcast, mpsc},
    time,
};
use tokio_stream::wrappers::{BroadcastStream, ReceiverStream};
use tokio_util::sync::CancellationToken;
use tracing::{instrument, trace, warn};

use crate::bus::{Command, CommandBus, Event, EventBus, Query, QueryBus};

#[derive(Default, Clone)]
pub struct InMemoryEventBus {
    inner: Arc<DashMap<TypeId, Box<dyn std::any::Any + Send + Sync>>>,
}

impl InMemoryEventBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    fn sender<E: Event>(&self) -> broadcast::Sender<Vec<u8>> {
        let key = TypeId::of::<E>();
        if let Some(existing) = self.inner.get(&key) {
            if let Some(tx) = existing
                .value()
                .downcast_ref::<broadcast::Sender<Vec<u8>>>()
            {
                return tx.clone();
            }
        }
        let (tx, _rx) = broadcast::channel::<Vec<u8>>(1024);
        self.inner.insert(key, Box::new(tx.clone()));
        tx
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    #[instrument(level = "debug", skip(self, evt))]
    async fn publish<E: Event>(&self, evt: E, timeout: Option<Duration>) -> Result<()> {
        let tx = self.sender::<E>();
        let bytes = serde_json::to_vec(&evt)?;
        // emit event type and current handler/subscriber count
        let n = tx.receiver_count();
        trace!(target = "airframe_event", event = %E::NAME, handlers = %n, "dispatch");
        if let Some(dur) = timeout {
            time::timeout(dur, async {
                let _ = tx.send(bytes);
            })
            .await?;
        } else {
            let _ = tx.send(bytes);
        }
        Ok(())
    }
    #[instrument(level = "debug", skip(self))]
    fn subscribe<E: Event>(&self) -> Result<ReceiverStream<E>> {
        let tx = self.sender::<E>();
        let rx = tx.subscribe();
        // Convert BroadcastStream<Result<Vec<u8>, RecvError>> into mpsc Receiver<E>
        let (out_tx, out_rx) = mpsc::channel(1024);
        tokio::spawn(async move {
            let mut bs = BroadcastStream::new(rx);
            while let Some(item) = bs.next().await {
                if let Ok(bytes) = item {
                    if let Ok(e) = serde_json::from_slice::<E>(&bytes) {
                        if out_tx.send(e).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(ReceiverStream::new(out_rx))
    }
}

#[derive(Default, Clone)]
pub struct InMemoryCommandBus {
    #[allow(clippy::type_complexity)]
    handlers: Arc<
        DashMap<
            TypeId,
            Arc<
                dyn Fn(
                        Box<dyn std::any::Any + Send>,
                        CancellationToken,
                    ) -> BoxFuture<'static, Result<()>>
                    + Send
                    + Sync,
            >,
        >,
    >,
}

#[async_trait]
impl CommandBus for InMemoryCommandBus {
    #[instrument(level = "debug", skip(self, cmd))]
    async fn dispatch<C: Command>(&self, cmd: C, timeout: Option<Duration>) -> Result<()> {
        let key = TypeId::of::<C>();
        let Some(h) = self.handlers.get(&key) else {
            return Err(anyhow!("no handler for command"));
        };
        let cancel = CancellationToken::new();
        let fut = (h)(Box::new(cmd), cancel);
        let start = Instant::now();
        if let Some(d) = timeout {
            time::timeout(d, fut).await?
        } else {
            fut.await
        }?;
        let elapsed = start.elapsed();
        // warn on slow handlers; do not log payloads
        if elapsed >= Duration::from_millis(500) {
            warn!(target = "airframe_event", handler = %C::NAME, elapsed_ms = %elapsed.as_millis(), elapsed_ns = %elapsed.as_nanos(), "slow handler");
        }
        Ok(())
    }

    fn register_handler<C, F>(&self, handler: F) -> Result<()>
    where
        C: Command,
        F: Fn(C, CancellationToken) -> BoxFuture<'static, Result<()>> + Send + Sync + 'static,
    {
        let key = TypeId::of::<C>();
        let wrapped = Arc::new(
            move |boxed: Box<dyn std::any::Any + Send>, cancel: CancellationToken| {
                let Ok(cmd) = boxed.downcast::<C>() else {
                    return async { Err(anyhow!("bad command type")) }.boxed();
                };
                handler(*cmd, cancel)
            },
        )
            as Arc<
                dyn Fn(
                        Box<dyn std::any::Any + Send>,
                        CancellationToken,
                    ) -> BoxFuture<'static, Result<()>>
                    + Send
                    + Sync,
            >;
        self.handlers.insert(key, wrapped);
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct InMemoryQueryBus {
    #[allow(clippy::type_complexity)]
    handlers: Arc<
        DashMap<
            TypeId,
            Arc<
                dyn Fn(
                        Box<dyn std::any::Any + Send>,
                        CancellationToken,
                    )
                        -> BoxFuture<'static, Result<Box<dyn std::any::Any + Send + Sync>>>
                    + Send
                    + Sync,
            >,
        >,
    >,
}

#[async_trait]
impl QueryBus for InMemoryQueryBus {
    #[instrument(level = "debug", skip(self, q))]
    async fn ask<Q: Query, R: Serialize + DeserializeOwned + Send + Sync + 'static>(
        &self,
        q: Q,
        timeout: Option<Duration>,
    ) -> Result<R> {
        let key = TypeId::of::<Q>();
        let Some(h) = self.handlers.get(&key) else {
            return Err(anyhow!("no handler for query"));
        };
        let cancel = CancellationToken::new();
        let fut = (h)(Box::new(q), cancel);
        let start = Instant::now();
        let boxed = if let Some(d) = timeout {
            time::timeout(d, fut).await??
        } else {
            fut.await?
        };
        let elapsed = start.elapsed();
        if elapsed >= Duration::from_millis(500) {
            warn!(target = "airframe_event", handler = %Q::NAME, elapsed_ms = %elapsed.as_millis(), elapsed_ns = %elapsed.as_nanos(), "slow handler");
        }
        boxed
            .downcast::<R>()
            .map(|b| *b)
            .map_err(|_| anyhow!("bad query response type"))
    }

    fn register_handler<Q, R, F>(&self, handler: F) -> Result<()>
    where
        Q: Query,
        R: Serialize + DeserializeOwned + Send + Sync + 'static,
        F: Fn(Q, CancellationToken) -> BoxFuture<'static, Result<R>> + Send + Sync + 'static,
    {
        let key = TypeId::of::<Q>();
        let wrapped = Arc::new(
            move |boxed: Box<dyn std::any::Any + Send>, cancel: CancellationToken| {
                let Ok(q) = boxed.downcast::<Q>() else {
                    return async { Err(anyhow!("bad query type")) }.boxed();
                };
                handler(*q, cancel)
                    .map(|res| res.map(|r| Box::new(r) as Box<dyn std::any::Any + Send + Sync>))
                    .boxed()
            },
        )
            as Arc<
                dyn Fn(
                        Box<dyn std::any::Any + Send>,
                        CancellationToken,
                    )
                        -> BoxFuture<'static, Result<Box<dyn std::any::Any + Send + Sync>>>
                    + Send
                    + Sync,
            >;
        self.handlers.insert(key, wrapped);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct AppStarted;
    impl Event for AppStarted {
        const NAME: &'static str = "AppStarted";
    }

    #[tokio::test]
    async fn event_bus_publish_subscribe() {
        let bus = InMemoryEventBus::new();
        let mut rx = bus.subscribe::<AppStarted>().unwrap();
        bus.publish(AppStarted, None).await.unwrap();
        let got = rx.next().await.expect("one event");
        // No content to assert beyond type; ensure stream yields
        let _ = got;
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct RotateLogs;
    impl Command for RotateLogs {
        const NAME: &'static str = "RotateLogs";
    }

    #[tokio::test]
    async fn command_bus_dispatch() {
        let bus = InMemoryCommandBus::default();
        bus.register_handler::<RotateLogs, _>(|_c, _cancel| async { Ok(()) }.boxed())
            .unwrap();
        bus.dispatch(RotateLogs, None).await.unwrap();
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct GetAnswer;
    impl Query for GetAnswer {
        const NAME: &'static str = "GetAnswer";
    }

    #[tokio::test]
    async fn query_bus_ask() {
        let bus = InMemoryQueryBus::default();
        bus.register_handler::<GetAnswer, i32, _>(|_q, _cancel| async { Ok(42) }.boxed())
            .unwrap();
        let r: i32 = bus.ask::<GetAnswer, i32>(GetAnswer, None).await.unwrap();
        assert_eq!(r, 42);
    }
}
