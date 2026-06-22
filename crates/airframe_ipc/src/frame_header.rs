//! Shared memory frame header for NUI pixel data exchange.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Header offset -- pixel data starts after this.
pub const HEADER_SIZE: usize = 64;

/// Magic value to identify a valid NUI frame header.
pub const NUI_FRAME_MAGIC: u32 = 0x4E554946; // "NUIF"

/// Current frame header version.
pub const NUI_FRAME_VERSION: u32 = 1;

/// Frame header flags.
pub mod flags {
    /// The page content has been rendered at least once.
    pub const READY: u32 = 1 << 0;
    /// The host renderer is shutting down.
    pub const SHUTDOWN: u32 = 1 << 1;
}

/// `#[repr(C)]` frame synchronization header at the start of the shared
/// memory region.
///
/// Layout (64 bytes total):
/// ```text
/// Offset  Size  Field
///   0       4   magic (0x4E554946 = "NUIF")
///   4       4   version
///   8       4   width
///  12       4   height
///  16       8   frame_counter (AtomicU64)
///  24       4   flags (AtomicU32)
///  28      36   reserved (padding to 64 bytes)
/// ```
///
/// Pixel data (RGBA, 4 bytes/pixel, row-major) begins at offset 64.
#[repr(C)]
pub struct NuiFrameHeader {
    pub magic: u32,
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub frame_counter: AtomicU64,
    pub flags: AtomicU32,
    _reserved: [u8; 36],
}

impl NuiFrameHeader {
    /// Initialize a frame header at the given memory location.
    ///
    /// # Safety
    ///
    /// `ptr` must point to at least `HEADER_SIZE` bytes of valid,
    /// writable memory that is properly aligned for `NuiFrameHeader`.
    pub unsafe fn init(ptr: *mut u8, width: u32, height: u32) -> &'static mut Self {
        let header = unsafe { &mut *(ptr as *mut Self) };
        header.magic = NUI_FRAME_MAGIC;
        header.version = NUI_FRAME_VERSION;
        header.width = width;
        header.height = height;
        header.frame_counter = AtomicU64::new(0);
        header.flags = AtomicU32::new(0);
        header._reserved = [0u8; 36];
        header
    }

    /// Interpret an existing shared memory region as a frame header.
    ///
    /// # Safety
    ///
    /// `ptr` must point to at least `HEADER_SIZE` bytes of valid memory
    /// that was previously initialized with `init()`.
    pub unsafe fn from_ptr(ptr: *const u8) -> &'static Self {
        unsafe { &*(ptr as *const Self) }
    }

    /// Interpret an existing shared memory region as a mutable frame header.
    ///
    /// # Safety
    ///
    /// Same as `from_ptr`, but requires mutable access.
    pub unsafe fn from_mut_ptr(ptr: *mut u8) -> &'static mut Self {
        unsafe { &mut *(ptr as *mut Self) }
    }

    /// Check if this header has valid magic and version.
    pub fn is_valid(&self) -> bool {
        self.magic == NUI_FRAME_MAGIC && self.version == NUI_FRAME_VERSION
    }

    /// Get the current frame counter value.
    pub fn frame_count(&self) -> u64 {
        self.frame_counter.load(Ordering::SeqCst)
    }

    /// Increment the frame counter and return the new value.
    pub fn increment_frame(&self) -> u64 {
        self.frame_counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get flags.
    pub fn get_flags(&self) -> u32 {
        self.flags.load(Ordering::SeqCst)
    }

    /// Set a flag.
    pub fn set_flag(&self, flag: u32) {
        self.flags.fetch_or(flag, Ordering::SeqCst);
    }

    /// Calculate the required shared memory size for the given dimensions.
    pub fn required_size(width: u32, height: u32) -> usize {
        HEADER_SIZE + (width as usize * height as usize * 4)
    }

    /// Get a pointer to the pixel data (offset 64).
    ///
    /// # Safety
    ///
    /// The caller must ensure the region is large enough.
    pub unsafe fn pixel_data_ptr(&self) -> *const u8 {
        unsafe { (self as *const Self as *const u8).add(HEADER_SIZE) }
    }

    /// Get a mutable pointer to the pixel data (offset 64).
    ///
    /// # Safety
    ///
    /// The caller must ensure the region is large enough and that no
    /// concurrent reads are happening.
    pub unsafe fn pixel_data_mut_ptr(&mut self) -> *mut u8 {
        unsafe { (self as *mut Self as *mut u8).add(HEADER_SIZE) }
    }
}

// Verify layout at compile time
const _: () = assert!(std::mem::size_of::<NuiFrameHeader>() == HEADER_SIZE);
const _: () = assert!(std::mem::align_of::<NuiFrameHeader>() <= 8);
