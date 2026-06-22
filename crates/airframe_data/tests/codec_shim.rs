#![cfg(feature = "codec-shim")]

use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::{BackendByteCache, Cache, CodecCache};
use airframe_data::key::Key;

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct Demo {
    a: u32,
    b: String,
}

#[test]
fn codec_shim_roundtrip_bincode() {
    let bytes = BackendByteCache::new(MemBackend::new());
    let ac = airframe_codec::codecs::BincodeCodec;
    let cache = CodecCache::new(ac, bytes);
    let k = Key::new("demo").unwrap();
    let v = Demo {
        a: 9,
        b: "nine".into(),
    };
    <CodecCache<_, _> as Cache<Demo>>::put(&cache, &k, &v).unwrap();
    let out: Demo = <CodecCache<_, _> as Cache<Demo>>::get(&cache, &k)
        .unwrap()
        .unwrap();
    assert_eq!(out, v);
}
