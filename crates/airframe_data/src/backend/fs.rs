use std::path::Path;

use crate::backend::fs_secure::{FsBackendSecure, FsSecureOptions};
use crate::backend::KvBackend;
use crate::error::Result;
use crate::key::Key;

/// Plain filesystem backend with atomic writes (temp file + rename).
///
/// This is a thin wrapper over [`FsBackendSecure`] with every hardening option
/// disabled — no permission hardening, no fsync durability, no directory sync.
/// Use [`FsBackendSecure`] directly when those guarantees are required. Keeping a
/// single underlying implementation avoids the two backends drifting apart (a
/// sanitization or atomicity fix to one now applies to both).
#[derive(Clone)]
pub struct FsBackend {
    inner: FsBackendSecure,
}

impl FsBackend {
    pub fn new<P: AsRef<Path>>(root: P, file_extension: &str) -> Result<Self> {
        let inner = FsBackendSecure::with_options(
            root,
            file_extension,
            FsSecureOptions {
                durable_writes: false,
                dirsync: false,
                hardened_permissions: false,
                harden_directories: false,
            },
        )?;
        Ok(FsBackend { inner })
    }

    #[cfg(test)]
    fn path_for(&self, key: &Key) -> std::path::PathBuf {
        self.inner.path_for(key)
    }
}

impl KvBackend for FsBackend {
    fn put_bytes(&self, key: &Key, buf: &[u8]) -> Result<()> {
        self.inner.put_bytes(key, buf)
    }

    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>> {
        self.inner.get_bytes(key)
    }

    fn remove(&self, key: &Key) -> Result<()> {
        self.inner.remove(key)
    }

    fn contains(&self, key: &Key) -> Result<bool> {
        self.inner.contains(key)
    }

    fn list(&self) -> Result<Vec<Key>> {
        self.inner.list()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn new_creates_directory_if_missing() {
        let tmp = tempdir().unwrap();
        let subdir = tmp.path().join("subdir");
        assert!(!subdir.exists());

        let _backend = FsBackend::new(&subdir, "dat").unwrap();
        assert!(subdir.exists());
    }

    #[test]
    fn put_and_get_bytes() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "dat").unwrap();
        let key = Key::new("item1").unwrap();
        let data = b"hello world";

        backend.put_bytes(&key, data).unwrap();
        let retrieved = backend.get_bytes(&key).unwrap();

        assert_eq!(retrieved, Some(data.to_vec()));
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "dat").unwrap();
        let key = Key::new("nonexistent").unwrap();

        let result = backend.get_bytes(&key).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn contains_returns_true_for_existing_key() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "dat").unwrap();
        let key = Key::new("exists").unwrap();

        assert!(!backend.contains(&key).unwrap());
        backend.put_bytes(&key, b"data").unwrap();
        assert!(backend.contains(&key).unwrap());
    }

    #[test]
    fn remove_deletes_file() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "dat").unwrap();
        let key = Key::new("to_remove").unwrap();

        backend.put_bytes(&key, b"data").unwrap();
        assert!(backend.contains(&key).unwrap());

        backend.remove(&key).unwrap();
        assert!(!backend.contains(&key).unwrap());
    }

    #[test]
    fn remove_nonexistent_is_ok() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "dat").unwrap();
        let key = Key::new("never_existed").unwrap();

        // Should not error
        backend.remove(&key).unwrap();
    }

    #[test]
    fn list_returns_stored_keys() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "dat").unwrap();

        let key1 = Key::new("item1").unwrap();
        let key2 = Key::new("item2").unwrap();

        backend.put_bytes(&key1, b"data1").unwrap();
        backend.put_bytes(&key2, b"data2").unwrap();

        let keys = backend.list().unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn list_empty_dir_returns_empty() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "dat").unwrap();

        let keys = backend.list().unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn extension_is_stripped_from_dots() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), ".json").unwrap();
        let key = Key::new("item").unwrap();

        backend.put_bytes(&key, b"{}").unwrap();

        // Should use "json" not ".json"
        let path = backend.path_for(&key);
        assert!(path.to_string_lossy().ends_with(".json"));
        assert!(!path.to_string_lossy().ends_with("..json"));
    }

    #[test]
    fn empty_extension_works() {
        let tmp = tempdir().unwrap();
        let backend = FsBackend::new(tmp.path(), "").unwrap();
        let key = Key::new("noext").unwrap();

        backend.put_bytes(&key, b"data").unwrap();
        let retrieved = backend.get_bytes(&key).unwrap();

        assert_eq!(retrieved, Some(b"data".to_vec()));
    }
}
