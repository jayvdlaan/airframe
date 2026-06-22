/// Windows Registry adapter crate.
/// Provides a ByteCache implementation backed by Windows Registry values.
pub const CRATE: &str = "airframe_winreg";

pub mod winreg_cache;
pub use winreg_cache::{HiveKind, WinRegByteCache};

#[cfg(target_os = "windows")]
pub mod module;
#[cfg(target_os = "windows")]
pub use module::{ServiceRegistryWinRegExt, WinRegModule};

pub fn ping() -> bool {
    true
}
