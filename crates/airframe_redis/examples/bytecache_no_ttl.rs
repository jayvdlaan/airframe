use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_redis::RedisByteCacheBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Requires a running Redis at 127.0.0.1:6379
    let bytes = RedisByteCacheBuilder::new("redis://127.0.0.1/")
        .namespace("no_ttl")
        .build()?;

    let k = Key::new("k1")?;
    bytes.put_bytes(&k, b"value")?;
    assert!(bytes.contains(&k)?);
    let out = bytes.get_bytes(&k)?.unwrap();
    assert_eq!(out, b"value");

    println!("redis no_ttl ok");
    Ok(())
}
