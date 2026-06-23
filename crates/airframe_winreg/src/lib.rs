//! Windows Registry-backed `ByteCache` provider for Airframe.
//!
//! `airframe_winreg` implements `airframe_data`'s `ByteCache` using the Windows
//! Registry, storing values as `REG_BINARY` under a configurable root path in
//! `HKCU` or `HKLM`. The adapter is synchronous and intentionally minimal.
//! Windows-only.
//!
//! # Key pieces
//! - [`WinRegByteCache`] — the registry-backed cache.
//! - [`HiveKind`] — selects `HKEY_CURRENT_USER` or `HKEY_LOCAL_MACHINE`.
//! - `WinRegModule` — Airframe module providing `cap:cache.winreg` (Windows-only).
//!
//! # Example
//! ```ignore
//! use airframe_winreg::{WinRegByteCache, HiveKind};
//!
//! let cache = WinRegByteCache::new(HiveKind::CurrentUser, r"Software\MyApp\Cache".into());
//! ```

pub mod winreg_cache;
pub use winreg_cache::{HiveKind, WinRegByteCache};

#[cfg(target_os = "windows")]
pub mod module;
#[cfg(target_os = "windows")]
pub use module::{ServiceRegistryWinRegExt, WinRegModule};
