//! Composable storage building blocks for Airframe.
//!
//! `airframe_data` provides small, predictable pieces for data storage: byte
//! backends, typed repositories, layered caches, codecs, and validated keys.
//! Higher layers (`airframe_pdata`, `airframe_secrets`, `airframe_sdata`) build
//! on these.
//!
//! # Key pieces
//! - [`backend`] — low-level byte backends (in-memory, filesystem, …).
//! - [`cache`] — the `ByteCache` abstraction and layered caches.
//! - [`repo`] — typed repositories over a backend.
//! - [`key`] — the validated storage [`Key`](key::Key) type.
//! - [`codec`] — serialization glue for typed storage.
//!
//! # Example
//! ```ignore
//! use airframe_data::key::Key;
//!
//! let key = Key::new("user:42")?; // rejects empty / "." / ".." segments
//! ```
pub mod backend;
pub mod cache;
pub mod codec;
pub mod error;
pub mod key;
pub mod repo;

#[cfg(test)]
mod tests {
    use crate::backend::{fs::FsBackend, mem::MemBackend};
    use crate::codec::JsonCodec;
    use crate::repo::{Repo, RepoBuilder};
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Profile {
        name: String,
        age: u8,
    }

    #[test]
    fn mem_store_roundtrip() {
        let backend = MemBackend::new();
        let codec = JsonCodec;
        let repo: Repo<_, _> = RepoBuilder::new()
            .backend(backend)
            .codec(codec)
            .build()
            .unwrap();
        let key = crate::key::Key::new("user_alice").unwrap();
        let value = Profile {
            name: "Alice".into(),
            age: 30,
        };
        repo.put(&key, &value).unwrap();
        assert!(repo.contains(&key).unwrap());
        let loaded: Profile = repo.get(&key).unwrap().unwrap();
        assert_eq!(loaded, value);
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        repo.remove(&key).unwrap();
        assert!(!repo.contains(&key).unwrap());
    }

    #[test]
    fn fs_store_roundtrip() {
        let dir = tempdir().unwrap();
        let backend = FsBackend::new(dir.path(), "json").unwrap();
        let codec = JsonCodec;
        let repo: Repo<_, _> = RepoBuilder::new()
            .backend(backend)
            .codec(codec)
            .build()
            .unwrap();
        let key = crate::key::Key::new("user_bob").unwrap();
        let value = Profile {
            name: "Bob".into(),
            age: 42,
        };
        repo.put(&key, &value).unwrap();
        let loaded: Profile = repo.get(&key).unwrap().unwrap();
        assert_eq!(loaded, value);
    }
}
