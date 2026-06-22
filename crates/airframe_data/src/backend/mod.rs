use crate::error::Result;
use crate::key::Key;

pub trait KvBackend: Send + Sync + 'static {
    fn put_bytes(&self, key: &Key, bytes: &[u8]) -> Result<()>;
    fn get_bytes(&self, key: &Key) -> Result<Option<Vec<u8>>>;
    fn remove(&self, key: &Key) -> Result<()>;
    fn contains(&self, key: &Key) -> Result<bool>;
    fn list(&self) -> Result<Vec<Key>>;
}

pub mod fs;
pub mod fs_secure;
pub mod mem;
