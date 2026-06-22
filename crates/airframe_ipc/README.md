# airframe_ipc

Concrete POSIX IPC primitives — shared memory, Unix domain sockets, and child processes — implementing the `spacetime_ipc` traits.

## Overview

`airframe_ipc` provides the host-side, `std`-backed implementations of the abstract IPC traits defined in `spacetime-ipc`. It is the Unix/host counterpart to the platform-neutral IPC contracts, used to wire a host renderer to a child process and exchange pixel frames over shared memory.

It exposes four public types, each implementing a `spacetime_ipc` trait:

- `MmapSharedRegion` (`shm`) — a POSIX shared memory region backed by `/dev/shm/`, created with `shm_open` + `ftruncate` + `mmap`. Implements `spacetime_ipc::SharedRegion`. The creator owns the region and `shm_unlink`s it on drop; openers attach non-owning. Regions are created exclusively with owner-only (`0o600`) permissions; a stale name is unlinked and recreated so the owner always holds a fresh, private region.
- `UnixSocketChannel` (`socket`) — a synchronous Unix domain socket channel with 4-byte big-endian length-prefixed framing. Implements `spacetime_ipc::IpcChannel` (`send` / `recv` / `poll`). Messages are capped at `MAX_MESSAGE_SIZE` (4 MB). `poll` peeks the socket via `recv(MSG_PEEK | MSG_DONTWAIT)`. A free `recv_vec` helper receives one message into a fresh `Vec`.
- `HostChildProcess` (`child`) — a child process handle wrapping `std::process::Child`. Implements `spacetime_ipc::ChildHandle` (`is_alive` via `/proc/{pid}`, `pid`, `kill` which also reaps the child).
- `NuiFrameHeader` (`frame_header`) — a `#[repr(C)]`, 64-byte frame synchronization header placed at the start of a shared region for NUI pixel-data exchange. Carries `magic` (`"NUIF"`), `version`, `width`/`height`, an atomic `frame_counter`, atomic `flags` (`READY`, `SHUTDOWN`), and reserved padding. Pixel data (RGBA, row-major) begins at offset 64. The layout is verified at compile time via `const` assertions.

## Airframe module compatibility

This crate is a standalone library and does **not** implement the Airframe `Module` / `ModuleDescriptor` system. It declares no capabilities and registers nothing in a `ServiceRegistry`. It implements only the `spacetime_ipc` traits and is consumed directly by callers that drive host/child IPC.

## Dependencies

- Internal: `spacetime-ipc` (`0.2.0-beta`, with the `std` feature) — provides the `SharedRegion`, `IpcChannel`, `ChildHandle` traits and the `IpcError` type.
- External:
  - `libc` (`0.2`, Unix only) — `shm_open`, `ftruncate`, `mmap`/`munmap`, `shm_unlink`, and the `recv(MSG_PEEK | MSG_DONTWAIT)` socket peek.
  - `thiserror` (`1.0`)
  - `tracing` (`0.1`)
- System: POSIX shared memory (`/dev/shm`) and Unix domain sockets. The crate is Unix-targeted (the `libc` dependency is gated on `cfg(unix)`).

## Usage

```rust
use airframe_ipc::{MmapSharedRegion, NuiFrameHeader, UnixSocketChannel};
use spacetime_ipc::{IpcChannel, SharedRegion};

// Owner side: create a shared region sized for a 1280x720 RGBA frame
// and initialize the synchronization header in place.
let width = 1280u32;
let height = 720u32;
let size = NuiFrameHeader::required_size(width, height);
let mut region = MmapSharedRegion::create("afterburner-nui-demo", size)?;

let header = unsafe { NuiFrameHeader::init(region.as_mut_ptr(), width, height) };
assert!(header.is_valid());
header.set_flag(airframe_ipc::frame_header::flags::READY);
let n = header.increment_frame();
assert_eq!(n, 1);

// A peer process attaches non-owning to the same region by name.
let _peer = MmapSharedRegion::open("afterburner-nui-demo", size)?;

// Control messages travel over a length-prefixed Unix socket channel.
let mut channel = UnixSocketChannel::connect("/tmp/afterburner.sock")?;
channel.send(b"hello")?;
let mut buf = [0u8; 64];
let _len = channel.recv(&mut buf)?;
# Ok::<(), spacetime_ipc::IpcError>(())
```

`HostChildProcess::spawn(program, args)` launches the host renderer, and `ChildHandle::is_alive` / `pid` / `kill` manage its lifecycle.

## Status

Pre-release (`0.5.0-beta`). The public API surface above is implemented; the crate targets Unix hosts.

Licensed under MIT.
