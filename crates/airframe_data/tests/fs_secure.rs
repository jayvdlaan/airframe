use airframe_data::backend::fs_secure::FsBackendSecure;
use airframe_data::backend::KvBackend;
use airframe_data::key::Key;
use tempfile::tempdir;

#[test]
fn fs_secure_roundtrip() {
    let dir = tempdir().unwrap();
    let backend = FsBackendSecure::new(dir.path(), "bin").unwrap();
    let key = Key::new("alpha").unwrap();
    let data = b"hello secure world".to_vec();

    backend.put_bytes(&key, &data).unwrap();
    assert!(backend.contains(&key).unwrap());
    let got = backend.get_bytes(&key).unwrap().unwrap();
    assert_eq!(got, data);

    let list = backend.list().unwrap();
    assert_eq!(list.len(), 1);

    backend.remove(&key).unwrap();
    assert!(!backend.contains(&key).unwrap());
}

#[cfg(unix)]
#[test]
fn fs_secure_permissions_unix() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let backend = FsBackendSecure::new(dir.path(), "dat").unwrap();
    let key = Key::new("perm").unwrap();
    let data = b"xyz".to_vec();

    backend.put_bytes(&key, &data).unwrap();

    // Check permissions are 0600
    let mut path = dir.path().to_path_buf();
    let mut name = key.encode_filename();
    name.push_str(".dat");
    path.push(name);

    let meta = fs::metadata(&path).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}
