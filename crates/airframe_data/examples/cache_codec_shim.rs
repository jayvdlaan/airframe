// Build with: cargo run -p airframe_data --example cache_codec_shim --features airframe_data/codec-shim

#[cfg(feature = "codec-shim")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use airframe_data::backend::mem::MemBackend;
    use airframe_data::cache::{BackendByteCache, Cache, CodecCache};
    use airframe_data::key::Key;

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct Demo {
        a: u32,
        b: String,
    }

    // Bytes cache over memory backend
    let bytes = BackendByteCache::new(MemBackend::new());

    // Use airframe_codec's BincodeCodec via the shimmed CodecCache
    let ac = airframe_codec::codecs::BincodeCodec;
    let cache = CodecCache::new(ac, bytes);

    let k = Key::new("demo:1")?;
    let v = Demo {
        a: 42,
        b: "answer".into(),
    };

    <CodecCache<_, _> as Cache<Demo>>::put(&cache, &k, &v)?;
    let out: Demo = <CodecCache<_, _> as Cache<Demo>>::get(&cache, &k)?.unwrap();
    assert_eq!(out, v);
    println!("cache_codec_shim ok: {:?}", out);
    Ok(())
}

#[cfg(not(feature = "codec-shim"))]
fn main() {
    println!(
        "Enable feature 'codec-shim' to run this example: --features airframe_data/codec-shim"
    );
}
