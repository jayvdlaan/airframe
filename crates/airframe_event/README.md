# airframe_event

Shared event contracts and a common `Tick` event layered on the `airframe_core` event bus.

## Overview

`airframe_event` is a thin contracts crate. It re-exports the `Event` marker trait and the
`EventBus` trait from `airframe_core::bus` so downstream crates can depend on a single, stable
event surface, and it ships one small concrete event (`Tick`) used by the crate's own tests and
by examples elsewhere in the workspace.

The actual bus implementations (`InMemoryEventBus`, command/query buses) live in
`airframe_core::bus` and `airframe_core::bus::inmem` — this crate does not redefine or wrap them.

What this crate actually provides:

- `Event` — re-export of `airframe_core::bus::Event` (a `Serialize + DeserializeOwned + Send +
  Sync + 'static` marker with an associated `const NAME: &'static str`).
- `EventBus` — re-export of `airframe_core::bus::EventBus` (the `publish` / `subscribe` trait).
- `Tick(pub u64)` — a serde-serializable counter event implementing `Event` (`NAME = "Tick"`).
- `CRATE: &str` — the crate identity string `"airframe_event"`.
- `ping() -> bool` — a readiness placeholder kept consistent across crates.

Note: the in-memory bus uses a Tokio broadcast channel internally, so subscribers only receive
events published after they subscribe; there is no replay of past events.

## Airframe module compatibility

This crate does not provide an Airframe module. It declares no `ModuleDescriptor`, no capabilities,
and registers nothing with the `ServiceRegistry`. It supplies event contracts and a shared event
type consumed by modules and binaries that use the bus.

## Dependencies

- `airframe_core` — source of the `Event` / `EventBus` traits and the in-memory bus.
- `serde` — derive support for serializable events.
- `thiserror` — error-type ergonomics.
- `tokio` — async runtime for publishing/subscribing.
- `futures` — `StreamExt` for consuming the `ReceiverStream` returned by `subscribe`.
- Dev: `serde_json` (round-trip tests).
- System libraries: none.
- Airframe capabilities/modules: none.

## Usage

```rust
use airframe_core::bus::inmem::InMemoryEventBus;
use airframe_event::{EventBus, Tick};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // InMemoryEventBus comes from airframe_core; airframe_event re-exports the EventBus trait.
    let bus = InMemoryEventBus::new();

    // Subscribe before publishing — broadcast semantics mean no replay of earlier events.
    let mut sub = bus.subscribe::<Tick>()?;

    // publish takes the event and an optional timeout (None = no timeout).
    bus.publish(Tick(7), None).await?;

    // subscribe returns a tokio_stream ReceiverStream; consume it via StreamExt::next().
    if let Some(Tick(n)) = sub.next().await {
        println!("got tick {n}");
    }
    Ok(())
}
```

Defining your own event is the same pattern as `Tick`: derive serde and implement `Event` with a
unique `NAME`.

```rust
use airframe_event::Event;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AppReady;

impl Event for AppReady {
    const NAME: &'static str = "AppReady";
}
```

## Status

Pre-release: `0.5.0-beta`. The public surface (re-exported `Event`/`EventBus`, the `Tick` event,
`CRATE`, and `ping`) is implemented and exercised by the crate's tests. This crate does not expose
an Airframe module.

Licensed under MIT.
