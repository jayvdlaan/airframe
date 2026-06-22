//! Zero-serialization typed event bus.
//!
//! Unlike the `InMemoryEventBus` which requires `Serialize + DeserializeOwned`,
//! the `TypedEventBus` works with any `Send + 'static` type directly,
//! avoiding serialization overhead entirely.

use std::any::{Any, TypeId};
use std::collections::VecDeque;
use std::sync::Mutex;

use dashmap::DashMap;

/// A high-performance, zero-serialization event bus.
///
/// Events are stored in per-type ring buffers and delivered to subscribers
/// via crossbeam-style channels. No `Serialize`/`DeserializeOwned` bounds required.
pub struct TypedEventBus {
    channels: DashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

struct TypedChannel<T: Send + 'static> {
    subscribers: Vec<tokio::sync::mpsc::UnboundedSender<T>>,
    buffer: VecDeque<T>,
    capacity: usize,
}

impl<T: Send + Clone + 'static> TypedChannel<T> {
    fn new(capacity: usize) -> Self {
        Self {
            subscribers: Vec::new(),
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn publish(&mut self, event: T) {
        // Notify all live subscribers
        self.subscribers.retain(|tx| tx.send(event.clone()).is_ok());

        // Store in ring buffer
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(event);
    }

    fn subscribe(&mut self) -> tokio::sync::mpsc::UnboundedReceiver<T> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.subscribers.push(tx);
        rx
    }

    fn drain(&mut self) -> Vec<T> {
        self.buffer.drain(..).collect()
    }
}

/// Receiver handle for typed events.
pub struct TypedReceiver<T: Send + 'static> {
    inner: tokio::sync::mpsc::UnboundedReceiver<T>,
}

impl<T: Send + 'static> TypedReceiver<T> {
    /// Receive the next event, waiting asynchronously.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }

    /// Try to receive an event without blocking.
    pub fn try_recv(&mut self) -> Option<T> {
        self.inner.try_recv().ok()
    }
}

impl Default for TypedEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl TypedEventBus {
    pub fn new() -> Self {
        Self {
            channels: DashMap::new(),
        }
    }

    /// Publish an event. All current subscribers receive a clone.
    /// The event is also stored in the per-type ring buffer.
    pub fn publish<T: Send + Clone + 'static>(&self, event: T) {
        let key = TypeId::of::<T>();
        let mut entry = self
            .channels
            .entry(key)
            .or_insert_with(|| Box::new(Mutex::new(TypedChannel::<T>::new(1024))));
        if let Some(channel) = entry.value_mut().downcast_mut::<Mutex<TypedChannel<T>>>() {
            let ch = channel.get_mut().unwrap();
            ch.publish(event);
        }
    }

    /// Publish an event with a custom ring buffer capacity (creates the channel if needed).
    pub fn publish_with_capacity<T: Send + Clone + 'static>(&self, event: T, capacity: usize) {
        let key = TypeId::of::<T>();
        let mut entry = self
            .channels
            .entry(key)
            .or_insert_with(|| Box::new(Mutex::new(TypedChannel::<T>::new(capacity))));
        if let Some(channel) = entry.value_mut().downcast_mut::<Mutex<TypedChannel<T>>>() {
            let ch = channel.get_mut().unwrap();
            ch.publish(event);
        }
    }

    /// Subscribe to events of type `T`. Returns a `TypedReceiver<T>`.
    pub fn subscribe<T: Send + Clone + 'static>(&self) -> TypedReceiver<T> {
        let key = TypeId::of::<T>();
        let mut entry = self
            .channels
            .entry(key)
            .or_insert_with(|| Box::new(Mutex::new(TypedChannel::<T>::new(1024))));
        let rx = if let Some(channel) = entry.value_mut().downcast_mut::<Mutex<TypedChannel<T>>>() {
            let ch = channel.get_mut().unwrap();
            ch.subscribe()
        } else {
            // Should never happen, but provide a disconnected receiver as fallback
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
            rx
        };
        TypedReceiver { inner: rx }
    }

    /// Drain all buffered events of type `T` from the ring buffer.
    pub fn drain<T: Send + Clone + 'static>(&self) -> Vec<T> {
        let key = TypeId::of::<T>();
        if let Some(mut entry) = self.channels.get_mut(&key) {
            if let Some(channel) = entry.value_mut().downcast_mut::<Mutex<TypedChannel<T>>>() {
                let ch = channel.get_mut().unwrap();
                return ch.drain();
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Debug, Clone, PartialEq)]
    struct PlayerJoined {
        name: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct PlayerLeft {
        id: u32,
    }

    #[tokio::test]
    async fn publish_subscribe_roundtrip() {
        let bus = TypedEventBus::new();
        let mut rx = bus.subscribe::<PlayerJoined>();

        bus.publish(PlayerJoined {
            name: "Alice".into(),
        });

        let got = rx.recv().await.expect("should receive event");
        assert_eq!(got.name, "Alice");
    }

    #[tokio::test]
    async fn multi_consumer() {
        let bus = TypedEventBus::new();
        let mut rx1 = bus.subscribe::<PlayerJoined>();
        let mut rx2 = bus.subscribe::<PlayerJoined>();

        bus.publish(PlayerJoined { name: "Bob".into() });

        let got1 = rx1.recv().await.unwrap();
        let got2 = rx2.recv().await.unwrap();
        assert_eq!(got1.name, "Bob");
        assert_eq!(got2.name, "Bob");
    }

    #[tokio::test]
    async fn different_event_types_isolated() {
        let bus = TypedEventBus::new();
        let mut rx_join = bus.subscribe::<PlayerJoined>();
        let mut rx_leave = bus.subscribe::<PlayerLeft>();

        bus.publish(PlayerJoined {
            name: "Charlie".into(),
        });
        bus.publish(PlayerLeft { id: 42 });

        let joined = rx_join.recv().await.unwrap();
        assert_eq!(joined.name, "Charlie");
        let left = rx_leave.recv().await.unwrap();
        assert_eq!(left.id, 42);
    }

    #[test]
    fn drain_returns_buffered_events() {
        let bus = TypedEventBus::new();
        bus.publish(PlayerJoined { name: "A".into() });
        bus.publish(PlayerJoined { name: "B".into() });

        let drained = bus.drain::<PlayerJoined>();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].name, "A");
        assert_eq!(drained[1].name, "B");

        // Second drain should be empty
        assert!(bus.drain::<PlayerJoined>().is_empty());
    }

    #[test]
    fn ring_buffer_overflow() {
        let bus = TypedEventBus::new();
        // Publish more than default capacity via a small custom capacity
        for i in 0..10 {
            bus.publish_with_capacity(PlayerLeft { id: i }, 5);
        }
        let drained = bus.drain::<PlayerLeft>();
        // Should only have the last 5 (ring buffer evicts oldest)
        assert_eq!(drained.len(), 5);
        assert_eq!(drained[0].id, 5);
        assert_eq!(drained[4].id, 9);
    }

    #[tokio::test]
    async fn concurrent_publish_subscribe() {
        let bus = Arc::new(TypedEventBus::new());
        let counter = Arc::new(AtomicUsize::new(0));
        let mut rx = bus.subscribe::<PlayerJoined>();

        let bus_clone = bus.clone();
        let publish_handle = tokio::spawn(async move {
            for i in 0..100 {
                bus_clone.publish(PlayerJoined {
                    name: format!("player-{i}"),
                });
            }
        });

        let counter_clone = counter.clone();
        let recv_handle = tokio::spawn(async move {
            while let Some(_event) = rx.recv().await {
                let prev = counter_clone.fetch_add(1, Ordering::Relaxed);
                if prev + 1 >= 100 {
                    break;
                }
            }
        });

        publish_handle.await.unwrap();
        recv_handle.await.unwrap();
        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn drain_empty_type_returns_empty() {
        let bus = TypedEventBus::new();
        let drained = bus.drain::<PlayerJoined>();
        assert!(drained.is_empty());
    }

    #[tokio::test]
    async fn try_recv_returns_none_when_empty() {
        let bus = TypedEventBus::new();
        let mut rx = bus.subscribe::<PlayerJoined>();
        assert!(rx.try_recv().is_none());
    }
}
