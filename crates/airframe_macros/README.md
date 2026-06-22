# airframe_macros

Declarative macros for Airframe module development.

## Overview

`airframe_macros` provides `macro_rules!` helpers that reduce boilerplate when
building Airframe modules. The macros construct `ModuleDescriptor` instances,
register services in a `ServiceRegistry`, generate typed service-access
extension traits, and convert typed `Cap` values into capability string slices.

The crate re-exports the `airframe_core` types the macros expand to (under
`$crate::`) so callers only need to depend on `airframe_macros` itself.

## Macros

### `module_descriptor!`

Build a `ModuleDescriptor` with named fields. `name` and `version` are required;
`provides`, `requires`, and `optional_requires` are optional and default to
empty.

```rust
use airframe_macros::module_descriptor;

let desc = module_descriptor!(
    name: "http_server",
    version: "2.1.0",
    provides: ["cap:http.server", "cap:router"],
    requires: ["cap:config", "cap:logging"],
    optional_requires: ["cap:metrics"],
);
```

### `register_service!`

Wrap a service in `Arc` and register it in a `ServiceRegistry`, optionally under
one or more trait object types.

```rust
use airframe_macros::register_service;

register_service!(registry, MyService::new());
register_service!(registry, my_service => [dyn MyTrait, dyn OtherTrait]);
```

### `service_ext!`

Generate an extension trait that adds typed getter methods to a registry type,
each delegating to `registry.get::<T>()`.

```rust
use airframe_macros::service_ext;
use airframe_core::registry::ServiceRegistry;

service_ext!(ServiceRegistryExt for ServiceRegistry {
    fn config(&self) -> Option<Arc<dyn ConfigProvider>>;
    fn logger(&self) -> Option<Arc<dyn Logger>>;
});

let config = registry.config();
```

### `caps!`

Convert typed `Cap` values into a `&[&str]` slice of their inner capability
strings.

```rust
use airframe_macros::caps;
use airframe_core::module::{CAP_HTTP_SERVER, CAP_CONFIG};

let provides: &[&str] = caps![CAP_HTTP_SERVER, CAP_CONFIG];
// == &["cap:http.server", "cap:config"]
```

## Dependencies

- `airframe_core` — provides `ModuleDescriptor`, `ServiceRegistry`, and the
  `Cap` capability types the macros expand to and re-export.

Dev-only: `semver` (used by `module_descriptor!` expansion in tests) and
`tokio`.

## Status

Pre-release (`0.5.0-beta`). Used across Airframe modules; API may change before
the stable release.

Licensed under MIT.
