use airframe_data::cache::ByteCache;
use airframe_data::error::{AirframeDataError, Result as DataResult};
use airframe_data::key::Key;

#[derive(Clone, Debug)]
pub enum HiveKind {
    CurrentUser,
    LocalMachine,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    #[derive(Clone)]
    pub struct WinRegByteCache {
        hive: HiveKind,
        root_path: String,
    }

    impl WinRegByteCache {
        pub fn new(hive: HiveKind, root_path: impl Into<String>) -> Self {
            Self {
                hive,
                root_path: root_path.into(),
            }
        }

        fn open_root_read(&self) -> std::io::Result<RegKey> {
            let (hkey, access) = match self.hive {
                HiveKind::CurrentUser => (HKEY_CURRENT_USER, KEY_READ | KEY_QUERY_VALUE),
                HiveKind::LocalMachine => (HKEY_LOCAL_MACHINE, KEY_READ | KEY_QUERY_VALUE),
            };
            let base = RegKey::predef(hkey);
            base.open_subkey_with_flags(&self.root_path, access)
        }

        fn open_root_write(&self) -> std::io::Result<(RegKey, RegKey)> {
            let (hkey, access) = match self.hive {
                HiveKind::CurrentUser => (HKEY_CURRENT_USER, KEY_READ | KEY_WRITE | KEY_SET_VALUE),
                HiveKind::LocalMachine => {
                    (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WRITE | KEY_SET_VALUE)
                }
            };
            let base = RegKey::predef(hkey);
            // Ensure the subkey exists
            base.create_subkey_with_flags(&self.root_path, access)
        }

        fn to_io(e: winreg::Error) -> AirframeDataError {
            AirframeDataError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        }
    }

    impl ByteCache for WinRegByteCache {
        fn put_bytes(&self, key: &Key, bytes: &[u8]) -> DataResult<()> {
            let (root, _disp) = self.open_root_write().map_err(Self::to_io)?;
            let rv = RegValue {
                vtype: REG_BINARY,
                bytes: bytes.to_vec(),
            };
            root.set_raw_value(key.as_str(), &rv).map_err(Self::to_io)?;
            Ok(())
        }

        fn get_bytes(&self, key: &Key) -> DataResult<Option<Vec<u8>>> {
            let root = match self.open_root_read() {
                Ok(k) => k,
                Err(_e) => return Ok(None),
            };
            match root.get_raw_value(key.as_str()) {
                Ok(rv) => {
                    if rv.vtype == REG_BINARY {
                        Ok(Some(rv.bytes))
                    } else {
                        Err(AirframeDataError::InvalidState)
                    }
                }
                Err(e) => {
                    // Map not found to None; others to error
                    match e.kind() {
                        winreg::ErrorKind::NotFound => Ok(None),
                        _ => Err(Self::to_io(e)),
                    }
                }
            }
        }

        fn remove(&self, key: &Key) -> DataResult<()> {
            let (root, _disp) = self.open_root_write().map_err(Self::to_io)?;
            match root.delete_value(key.as_str()) {
                Ok(()) => Ok(()),
                Err(e) => match e.kind() {
                    winreg::ErrorKind::NotFound => Ok(()),
                    _ => Err(Self::to_io(e)),
                },
            }
        }

        fn contains(&self, key: &Key) -> DataResult<bool> {
            let root = match self.open_root_read() {
                Ok(k) => k,
                Err(_e) => return Ok(false),
            };
            match root.get_raw_value(key.as_str()) {
                Ok(_rv) => Ok(true),
                Err(e) => match e.kind() {
                    winreg::ErrorKind::NotFound => Ok(false),
                    _ => Err(Self::to_io(e)),
                },
            }
        }

        fn list(&self) -> DataResult<Vec<Key>> {
            let root = match self.open_root_read() {
                Ok(k) => k,
                Err(_e) => return Ok(Vec::new()),
            };
            let mut out = Vec::new();
            for item in root.enum_values() {
                match item {
                    Ok((name, _rv)) => {
                        if let Ok(k) = Key::new(&name) {
                            out.push(k);
                        }
                    }
                    Err(_e) => { /* skip invalid */ }
                }
            }
            Ok(out)
        }
    }

    // Re-export in windows module
    pub use WinRegByteCache as PlatformWinRegByteCache;
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    #[derive(Clone)]
    pub struct WinRegByteCache {
        _phantom: (),
    }

    impl WinRegByteCache {
        pub fn new(_hive: HiveKind, _root_path: impl Into<String>) -> Self {
            Self { _phantom: () }
        }
    }

    impl ByteCache for WinRegByteCache {
        fn put_bytes(&self, _key: &Key, _bytes: &[u8]) -> DataResult<()> {
            Err(AirframeDataError::InvalidState)
        }
        fn get_bytes(&self, _key: &Key) -> DataResult<Option<Vec<u8>>> {
            Err(AirframeDataError::InvalidState)
        }
        fn remove(&self, _key: &Key) -> DataResult<()> {
            Err(AirframeDataError::InvalidState)
        }
        fn contains(&self, _key: &Key) -> DataResult<bool> {
            Err(AirframeDataError::InvalidState)
        }
        fn list(&self) -> DataResult<Vec<Key>> {
            Err(AirframeDataError::InvalidState)
        }
    }

    pub use WinRegByteCache as PlatformWinRegByteCache;
}

pub use imp::PlatformWinRegByteCache as WinRegByteCache;

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_data::cache::ByteCache;

    #[cfg(not(windows))]
    #[test]
    fn non_windows_stubs_error() {
        let cache = WinRegByteCache::new(HiveKind::CurrentUser, "any");
        let k = Key::new("k").unwrap();
        // All operations should be InvalidState
        assert!(matches!(
            cache.put_bytes(&k, b"1"),
            Err(AirframeDataError::InvalidState)
        ));
        assert!(matches!(
            cache.get_bytes(&k),
            Err(AirframeDataError::InvalidState)
        ));
        assert!(matches!(
            cache.remove(&k),
            Err(AirframeDataError::InvalidState)
        ));
        assert!(matches!(
            cache.contains(&k),
            Err(AirframeDataError::InvalidState)
        ));
        assert!(matches!(cache.list(), Err(AirframeDataError::InvalidState)));
    }

    #[cfg(windows)]
    mod win {
        use super::*;
        use winreg::enums::*;
        use winreg::RegKey;

        fn test_cache() -> WinRegByteCache {
            WinRegByteCache::new(HiveKind::CurrentUser, r"Software\\Airframe\\WinregTest")
        }

        fn cleanup_root() {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let _ = hkcu.delete_subkey_all(r"Software\Airframe\WinregTest");
        }

        #[test]
        fn put_get_remove_and_contains() {
            cleanup_root();
            let cache = test_cache();
            let k = Key::new("hello").unwrap();
            assert!(!cache.contains(&k).unwrap());
            cache.put_bytes(&k, b"world").unwrap();
            assert!(cache.contains(&k).unwrap());
            let got = cache.get_bytes(&k).unwrap().unwrap();
            assert_eq!(got, b"world");
            cache.remove(&k).unwrap();
            assert!(!cache.contains(&k).unwrap());
            // get on missing -> Ok(None)
            assert!(cache.get_bytes(&k).unwrap().is_none());
        }

        #[test]
        fn list_returns_only_valid_keys() {
            cleanup_root();
            let cache = test_cache();
            let a = Key::new("a").unwrap();
            let b = Key::new("b").unwrap();
            cache.put_bytes(&a, b"1").unwrap();
            cache.put_bytes(&b, b"2").unwrap();
            let mut list = cache.list().unwrap();
            list.sort_by(|x, y| x.as_str().cmp(y.as_str()));
            let names: Vec<String> = list.into_iter().map(|k| k.to_string()).collect();
            assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
            cleanup_root();
        }

        #[test]
        fn non_binary_values_error() {
            cleanup_root();
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (root, _disp) = hkcu.create_subkey(r"Software\Airframe\WinregTest").unwrap();
            // write a string value directly
            root.set_value("str_value", &"hi").unwrap();

            let cache = test_cache();
            let k = Key::new("str_value").unwrap();
            let err = cache.get_bytes(&k).unwrap_err();
            assert!(matches!(err, AirframeDataError::InvalidState));
            cleanup_root();
        }
    }
}
