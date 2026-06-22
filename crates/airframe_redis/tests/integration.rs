#![cfg(any(feature = "test-integration", feature = "integration"))]

use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_redis::RedisByteCacheBuilder;
use std::time::Duration;

#[test]
fn roundtrip_and_ttl() {
    // Requires local Redis; skip if cannot connect
    let builder = RedisByteCacheBuilder::new("redis://127.0.0.1/")
        .namespace("it")
        .default_ttl(Duration::from_millis(200));
    let cache = match builder.build() {
        Ok(c) => c,
        Err(_) => return, // skip
    };
    let k = Key::new("x").unwrap();
    cache.put_bytes(&k, b"v").unwrap();
    assert!(cache.contains(&k).unwrap());
    std::thread::sleep(Duration::from_millis(250));
    assert!(!cache.contains(&k).unwrap());
}
