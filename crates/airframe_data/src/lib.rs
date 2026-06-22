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
