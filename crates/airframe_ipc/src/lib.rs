//! Concrete IPC implementations for shared memory, Unix domain sockets,
//! and child process management.
//!
//! These types implement the traits from `spacetime_ipc`.

pub mod child;
pub mod frame_header;
pub mod shm;
pub mod socket;

pub use child::HostChildProcess;
pub use frame_header::NuiFrameHeader;
pub use shm::MmapSharedRegion;
pub use socket::UnixSocketChannel;
