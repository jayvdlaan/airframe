#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;

use airframe_data::backend::fs_secure::{FsBackendSecure, FsSecureOptions};
use airframe_data::backend::KvBackend;
use airframe_data::key::Key;

#[test]
fn fs_secure_harden_directories_sets_0700() {
    let root = tempdir().unwrap();
    let path = root.path().join("nested");
    let opts = FsSecureOptions {
        harden_directories: true,
        ..Default::default()
    };

    let backend = FsBackendSecure::with_options(&path, "bin", opts).unwrap();
    // Trigger parent creation and write
    let key = Key::new("dirperm").unwrap();
    backend.put_bytes(&key, b"abc").unwrap();

    // Check root (nested) directory permissions are 0700
    let meta = fs::metadata(&path).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}
