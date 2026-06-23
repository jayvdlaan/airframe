# airframe_core

## Overview

Core contracts and in-memory runtime for the modular Airframe architecture.

Includes:
- Module + ModuleDescriptor + ModuleContext
- ServiceRegistry for service discovery by type
- In-memory Event/Command/Query buses
- AppBuilder/AppHandle to assemble modules with simple dependency resolution

## Logical pieces

- Module: trait implemented by crates that plug into the runtime; described by ModuleDescriptor and initialized with ModuleContext
- ServiceRegistry: type-indexed container for registering and fetching services at runtime
- Buses: in-memory EventBus, CommandBus, and QueryBus for intra-app messaging
- AppBuilder/AppHandle: construct and start an app, wire modules with simple dependency ordering, and access services/buses at runtime

## Airframe module compatibility

- Compatibility: N/A (this crate provides the module system and runtime used by other crates)

## Dependencies

- Rust dependencies: see Cargo.toml
- System libraries: none
- Airframe capacities/modules: Provides the Airframe module system and runtime; it does not export a specific capability.

## Setup / Installation

```toml
[dependencies]
airframe_core = { path = "../airframe_core" }
```

## Usage & Examples

- See `examples/basic_app.rs` for a minimal example composing Args, Config, and KV modules.
- See `examples/end_to_end_app.rs` for an end-to-end example demonstrating Args + layered Config + Logging updates + KV usage + Scheduler JobSpec from KV + Health AppReady.
- See `examples/minimal_router.rs` for a tiny example of a module contributing a router via ServiceRegistry.

### Run examples
- Basic app:
  - cargo run -q -p airframe_core --example basic_app
- End-to-end app:
  - cargo run -q -p airframe_core --example end_to_end_app
- Minimal router:
  - cargo run -q -p airframe_core --example minimal_router

### What to expect (end_to_end_app)
- Prints a GraphViz DOT of the module graph, e.g.:
  digraph modules {
    "args";
    "config";
    "kv";
    "scheduler";
    "logging";
    "health";
    "args" -> "(none)";
  }
  (Actual providers/edges depend on loaded modules.)
- Publishes AppReady once required Health checks become Healthy.
- Reacts to config file change by updating Logging and publishing LoggingChanged.
- Stores a KV key demo/hello and shows a Scheduler-driven ticks counter under scheduler/jobs/heartbeat/ticks.

## Features

The following optional features enable integrations with sibling Airframe crates when you want them, without imposing dependencies on consumers who only need core types:

```
[features]
default = []
logging = ["airframe_logging"]
config  = ["airframe_config"]
args    = ["airframe_args"]
```

These are primarily used by examples and tests; the core runtime does not require them.

## License

Licensed under the MIT License.
