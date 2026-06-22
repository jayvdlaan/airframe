use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::Builder as TempBuilder;

#[cfg(all(windows, feature = "windows-acl"))]
mod win {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, PWSTR};
    use windows::Win32::Security::{Authorization::*, *};
    use windows::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};
    use windows::Win32::System::Threading::GetCurrentProcess;

    fn to_wide_null(s: &Path) -> Vec<u16> {
        let os: &OsStr = s.as_os_str();
        let mut v: Vec<u16> = os.encode_wide().collect();
        v.push(0);
        v
    }

    pub fn set_restrictive_dacl(path: &Path) -> Result<(), crate::error::AirframeDataError> {
        unsafe {
            // Get current user SID from process token
            let process = GetCurrentProcess();
            let mut token: HANDLE = HANDLE::default();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
                return Err(io::Error::from_raw_os_error(GetLastError().0 as i32).into());
            }
            // First call to get size
            let mut needed = 0u32;
            let ok = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
            if ok.as_bool() || needed == 0 {
                // unexpected, but continue
            }
            let mut buf = vec![0u8; needed as usize];
            let ok = GetTokenInformation(
                token,
                TokenUser,
                Some(buf.as_mut_ptr() as *mut _),
                needed,
                &mut needed,
            );
            if !ok.as_bool() {
                let _ = CloseHandle(token);
                return Err(io::Error::from_raw_os_error(GetLastError().0 as i32).into());
            }
            let token_user: *const TOKEN_USER = buf.as_ptr() as *const TOKEN_USER;
            let sid_ptr = (*token_user).User.Sid;

            // Build EXPLICIT_ACCESS for current user: GENERIC_READ | GENERIC_WRITE
            let mut ea: EXPLICIT_ACCESS_W = std::mem::zeroed();
            ea.grfAccessPermissions = FILE_GENERIC_READ | FILE_GENERIC_WRITE;
            ea.grfAccessMode = SET_ACCESS;
            ea.grfInheritance = NO_INHERITANCE;
            ea.Trustee = TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: MULTIPLE_TRUSTEE_OPERATION(0),
                TrusteeForm: TRUSTEE_FORM(TrusteeForm::TRUSTEE_IS_SID.0 as i32),
                TrusteeType: TRUSTEE_TYPE(TrusteeType::TRUSTEE_IS_USER.0 as i32),
                ptstrName: PWSTR(sid_ptr as *mut _),
            };

            let mut new_dacl: *mut ACL = std::ptr::null_mut();
            let res = SetEntriesInAclW(Some(&[ea]), None, &mut new_dacl);
            if res.0 != 0 {
                let _ = CloseHandle(token);
                return Err(io::Error::from_raw_os_error(res.0 as i32).into());
            }

            // Apply DACL (and protect it from inheritance)
            let wide = to_wide_null(path);
            let res = SetNamedSecurityInfoW(
                PWSTR(wide.as_ptr() as *mut _),
                SE_OBJECT_TYPE::SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(new_dacl),
                None,
            );

            let _ = LocalFree(new_dacl as isize);
            let _ = CloseHandle(token);

            if res.0 != 0 {
                return Err(io::Error::from_raw_os_error(res.0 as i32).into());
            }
            Ok(())
        }
    }
}

use crate::backend::KvBackend;
use crate::error::Result;
use crate::key::Key;
use tracing::{error, instrument};

#[derive(Clone, Debug)]
pub struct FsSecureOptions {
    pub durable_writes: bool,
    pub dirsync: bool,
    pub hardened_permissions: bool,
    pub harden_directories: bool,
}

impl Default for FsSecureOptions {
    fn default() -> Self {
        Self {
            durable_writes: true,
            dirsync: true,
            hardened_permissions: true,
            harden_directories: false,
        }
    }
}

#[derive(Clone)]
pub struct FsBackendSecure {
    root: PathBuf,
    ext: String,
    options: FsSecureOptions,
}

impl FsBackendSecure {
    pub fn new<P: AsRef<Path>>(root: P, file_extension: &str) -> Result<Self> {
        Self::with_options(root, file_extension, FsSecureOptions::default())
    }

    pub fn with_options<P: AsRef<Path>>(
        root: P,
        file_extension: &str,
        options: FsSecureOptions,
    ) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.exists() {
            fs::create_dir_all(&root)?;
            if options.harden_directories {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perm = fs::Permissions::from_mode(0o700);
                    fs::set_permissions(&root, perm)?;
                }
                #[cfg(all(windows, feature = "windows-acl"))]
                {
                    if let Err(e) = crate::backend::fs_secure::win::set_restrictive_dacl(&root) {
                        return Err(e);
                    }
                }
            }
        }
        Ok(Self {
            root,
            ext: file_extension.trim_start_matches('.').to_string(),
            options,
        })
    }

    pub(crate) fn path_for(&self, key: &Key) -> PathBuf {
        let mut p = self.root.clone();
        let mut name = key.encode_filename();
        if !self.ext.is_empty() {
            name.push('.');
            name.push_str(&self.ext);
        }
        p.push(name);
        p
    }
}

impl KvBackend for FsBackendSecure {
    #[instrument(level = "debug", skip(self, buf))]
    fn put_bytes(&self, key: &Key, buf: &[u8]) -> Result<()> {
        let path = self.path_for(key);
        let parent = path.parent().unwrap_or(&self.root);
        if let Err(e) = fs::create_dir_all(parent) {
            error!(target = "airframe_data", key = %key, error = ?e, "write failed");
            return Err(e.into());
        }
        if self.options.harden_directories {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perm = fs::Permissions::from_mode(0o700);
                fs::set_permissions(parent, perm)?;
            }
            #[cfg(all(windows, feature = "windows-acl"))]
            {
                if let Err(e) = crate::backend::fs_secure::win::set_restrictive_dacl(parent) {
                    return Err(e);
                }
            }
        }

        // Create temp in same directory. tempfile creates 0o600 by default on Unix, but we also fix perms after persist
        let mut tmp = TempBuilder::new().tempfile_in(parent)?;

        if let Err(e) = tmp.write_all(buf) {
            error!(target = "airframe_data", key = %key, error = ?e, "write failed");
            return Err(e.into());
        }
        // Flush OS buffer for good measure (not a durability guarantee)
        if let Err(e) = tmp.flush() {
            error!(target = "airframe_data", key = %key, error = ?e, "write failed");
            return Err(e.into());
        }
        if self.options.durable_writes {
            // Ensure file contents reach the disk before rename
            if let Err(e) = tmp.as_file().sync_all() {
                error!(target = "airframe_data", key = %key, error = ?e, "write failed");
                return Err(e.into());
            }
        }

        // Atomic rename to final path
        let persisted = match tmp.persist(&path) {
            Ok(p) => p,
            Err(e) => {
                error!(target = "airframe_data", key = %key, error = ?e.error, "write failed");
                return Err(e.error.into());
            }
        };

        // Hardened permissions
        if self.options.hardened_permissions {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perm = fs::Permissions::from_mode(0o600);
                fs::set_permissions(&path, perm)?;
            }
            #[cfg(all(windows, feature = "windows-acl"))]
            {
                // Apply a restrictive DACL so only the current user has read/write access.
                if let Err(e) = crate::backend::fs_secure::win::set_restrictive_dacl(&path) {
                    // If ACL setting fails, return IO error to signal hardening failed.
                    error!(target = "airframe_data", key = %key, error = ?e, "write failed");
                    return Err(e);
                }
            }
            #[cfg(all(windows, not(feature = "windows-acl")))]
            {
                // Best-effort: Without the feature enabled, we cannot set a restrictive DACL programmatically.
                // Recommend securing the containing directory via ACLs. Proceed without changing permissions.
                let _ = &persisted;
            }
        }

        if self.options.dirsync {
            // Drop file handle before dirsync on some platforms
            drop(persisted);
            // Sync the parent directory to persist the rename
            let dir = File::open(parent)?;
            if let Err(e) = dir.sync_all() {
                error!(target = "airframe_data", key = %key, error = ?e, "write failed");
                return Err(e.into());
            }
        } else {
            drop(persisted);
        }

        Ok(())
    }

    #[instrument(level = "debug", skip(self))]
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        let path = self.path_for(key);
        if !path.exists() {
            return Ok(None);
        }
        let mut f = fs::File::open(path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(Some(buf))
    }

    #[instrument(level = "debug", skip(self))]
    fn remove(&self, key: &Key) -> Result<()> {
        let path = self.path_for(key);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    fn contains(&self, key: &Key) -> Result<bool> {
        Ok(self.path_for(key).exists())
    }

    fn list(&self) -> Result<Vec<Key>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                    let stem = if self.ext.is_empty() {
                        file_name
                    } else {
                        file_name
                            .strip_suffix(&format!(".{}", self.ext))
                            .unwrap_or(file_name)
                    };
                    if let Ok(k) = Key::decode_filename(stem) {
                        out.push(k);
                    }
                }
            }
        }
        Ok(out)
    }
}
