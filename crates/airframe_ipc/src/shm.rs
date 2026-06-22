//! POSIX shared memory region using `shm_open` + `mmap`.

use spacetime_ipc::{IpcError, SharedRegion};
use std::ffi::CString;
use std::os::unix::io::RawFd;

/// A POSIX shared memory region backed by `/dev/shm/`.
///
/// The region is named `/afterburner-nui-{suffix}` and can be opened by
/// multiple processes. Under Wine, the region is accessible via
/// `CreateFileA("/dev/shm/afterburner-nui-{suffix}")` followed by
/// `CreateFileMappingA` + `MapViewOfFile`.
pub struct MmapSharedRegion {
    ptr: *mut u8,
    size: usize,
    fd: RawFd,
    name: CString,
    owner: bool,
}

// SAFETY: The shared memory region is a raw pointer to a mapped page.
// Access must be synchronized externally (via atomics in NuiFrameHeader).
unsafe impl Send for MmapSharedRegion {}
unsafe impl Sync for MmapSharedRegion {}

impl MmapSharedRegion {
    /// Create a new shared memory region. The creator owns it and will
    /// unlink it on drop.
    pub fn create(name: &str, size: usize) -> Result<Self, IpcError> {
        let shm_name = format!("/{name}");
        let c_name = CString::new(shm_name).map_err(|_| IpcError::InvalidArgument)?;

        unsafe {
            // Create exclusively with owner-only permissions (0o600). If a region with
            // this name already exists (stale after a crash, or squatted by another
            // user), unlink it and retry exclusively so we always own a fresh, private
            // region rather than silently attaching to one we did not create.
            let mut fd = libc::shm_open(
                c_name.as_ptr(),
                libc::O_CREAT | libc::O_RDWR | libc::O_EXCL,
                0o600,
            );
            if fd < 0 {
                libc::shm_unlink(c_name.as_ptr());
                fd = libc::shm_open(
                    c_name.as_ptr(),
                    libc::O_CREAT | libc::O_RDWR | libc::O_EXCL,
                    0o600,
                );
            }
            if fd < 0 {
                return Err(IpcError::RegionCreateFailed);
            }
            Self::from_fd(fd, c_name, size, true)
        }
    }

    /// Open an existing shared memory region (non-owning).
    pub fn open(name: &str, size: usize) -> Result<Self, IpcError> {
        let shm_name = format!("/{name}");
        let c_name = CString::new(shm_name).map_err(|_| IpcError::InvalidArgument)?;

        unsafe {
            // No O_CREAT here, so the mode is ignored; attach read-write to the
            // region created by the owner (which is now 0o600, same-user only).
            let fd = libc::shm_open(c_name.as_ptr(), libc::O_RDWR, 0o600);
            if fd < 0 {
                return Err(IpcError::RegionCreateFailed);
            }
            Self::from_fd(fd, c_name, size, false)
        }
    }

    unsafe fn from_fd(
        fd: RawFd,
        name: CString,
        size: usize,
        owner: bool,
    ) -> Result<Self, IpcError> {
        // Set the size
        if libc::ftruncate(fd, size as libc::off_t) < 0 {
            libc::close(fd);
            if owner {
                libc::shm_unlink(name.as_ptr());
            }
            return Err(IpcError::RegionCreateFailed);
        }

        // Map the memory
        let ptr = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        if ptr == libc::MAP_FAILED {
            libc::close(fd);
            if owner {
                libc::shm_unlink(name.as_ptr());
            }
            return Err(IpcError::RegionMapFailed);
        }

        Ok(Self {
            ptr: ptr as *mut u8,
            size,
            fd,
            name,
            owner,
        })
    }

    /// Returns the shared memory name (without the leading `/`).
    pub fn name(&self) -> &str {
        // Skip the leading '/'
        let s = self.name.to_str().unwrap_or("");
        s.strip_prefix('/').unwrap_or(s)
    }
}

impl SharedRegion for MmapSharedRegion {
    fn as_ptr(&self) -> *const u8 {
        self.ptr as *const u8
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    fn len(&self) -> usize {
        self.size
    }
}

impl Drop for MmapSharedRegion {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.size);
            libc::close(self.fd);
            if self.owner {
                libc::shm_unlink(self.name.as_ptr());
            }
        }
    }
}
