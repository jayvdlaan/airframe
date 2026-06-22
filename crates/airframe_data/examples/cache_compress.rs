// Build with: cargo run -p airframe_data --example cache_compress --features airframe_data/integration-compress
#[cfg(feature = "integration-compress")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use airframe_data::cache::{CompressByteCache, MemByteCache};
    use airframe_data::key::Key;

    let algo = airframe_compress::Zstd::new(3);
    let bytes = CompressByteCache::new(MemByteCache::new(), algo);

    let k = Key::new("blob")?;
    let data: Vec<u8> = (0..50_000).map(|i| (i % 251) as u8).collect();
    bytes.put_bytes(&k, &data)?;
    let out = bytes.get_bytes(&k)?.unwrap();
    assert_eq!(out, data);

    println!("cache_compress ok");
    Ok(())
}

#[cfg(not(feature = "integration-compress"))]
fn main() {
    println!("Enable feature 'integration-compress' to run this example: --features airframe_data/integration-compress");
}
