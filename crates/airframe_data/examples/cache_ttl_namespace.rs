use airframe_data::cache::ByteCache;
use airframe_data::cache::{MemByteCache, NamespaceByteCache, TtlByteCache};
use airframe_data::key::Key;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = MemByteCache::new();
    let ttl = TtlByteCache::with_ttl(base, Duration::from_millis(200));
    let ns = NamespaceByteCache::new(ttl, "session");

    let k = Key::new("token")?;
    ns.put_bytes(&k, b"abc")?;
    assert_eq!(ns.get_bytes(&k)?, Some(b"abc".to_vec()));

    std::thread::sleep(Duration::from_millis(250));
    assert!(ns.get_bytes(&k)?.is_none());

    println!("cache_ttl_namespace ok");
    Ok(())
}
