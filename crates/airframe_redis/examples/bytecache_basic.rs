use airframe_data::cache::ByteCache;
use airframe_data::key::Key;
use airframe_redis::RedisByteCacheBuilder;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Requires a running Redis at 127.0.0.1:6379
    let bytes = RedisByteCacheBuilder::new("redis://127.0.0.1/")
        .namespace("demo")
        .default_ttl(Duration::from_secs(5))
        .build()?;

    let k = Key::new("hello")?;
    let payload = b"world".to_vec();
    bytes.put_bytes(&k, &payload)?;
    let got = bytes.get_bytes(&k)?.unwrap();
    assert_eq!(got, payload);

    let mut keys = bytes.list()?;
    keys.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    println!(
        "redis bytecache ok: keys={:?}",
        keys.iter().map(|k| k.as_str()).collect::<Vec<_>>()
    );
    Ok(())
}
