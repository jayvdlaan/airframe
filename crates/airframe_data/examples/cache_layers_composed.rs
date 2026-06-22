// Build with:
// Default features: requires --features airframe_data/integration-compress for compression layer
// cargo run -p airframe_data --example cache_layers_composed --features airframe_data/integration-compress

#[cfg(feature = "integration-compress")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use airframe_data::backend::fs::FsBackend;
    use airframe_data::cache::BackendByteCache;
    use airframe_data::cache::{Cache, SerdeCache};
    use airframe_data::cache::{
        CompressByteCache, LruByteCache, MemByteCache, NamespaceByteCache, ReadThroughByteCache,
        TtlByteCache,
    };
    use airframe_data::codec::{Codec, JsonCodec};
    use airframe_data::key::Key;
    use std::time::Duration;

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct User {
        id: u32,
        name: String,
    }

    // Backend: FS (cold storage)
    let tmp = tempfile::tempdir()?;
    let fs = FsBackend::new(tmp.path(), JsonCodec.file_extension())?;
    let back_bytes = BackendByteCache::new(fs);

    // Compose layers on the back store: Compress -> Namespace -> TTL
    let zstd = airframe_compress::Zstd::new(3);
    let compressed_back = CompressByteCache::new(back_bytes, zstd);
    let namespaced_back = NamespaceByteCache::new(compressed_back, "users");
    let ttl_back = TtlByteCache::with_ttl(namespaced_back, Duration::from_secs(5));

    // Front: LRU memory cache
    let front = LruByteCache::new(128);

    // Read-through cache combining front and back
    let two_level = ReadThroughByteCache::new(front, ttl_back);

    // Typed access using local serde-based codec
    let typed = SerdeCache::new(JsonCodec, two_level);

    let k = Key::new("u:1001")?;
    let v = User {
        id: 1001,
        name: "Luna".into(),
    };

    <_ as Cache<User>>::put(&typed, &k, &v)?;
    let out: User = <_ as Cache<User>>::get(&typed, &k)?.unwrap();
    assert_eq!(out, v);

    println!("cache_layers_composed ok: {:?}", out);
    Ok(())
}

#[cfg(not(feature = "integration-compress"))]
fn main() {
    println!("Enable feature 'integration-compress' to run this example: --features airframe_data/integration-compress");
}
