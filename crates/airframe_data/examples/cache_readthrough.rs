use airframe_data::cache::{LruByteCache, ReadThroughByteCache};
use airframe_data::{
    backend::fs::FsBackend,
    cache::{BackendByteCache, Cache, SerdeCache},
    codec::{Codec, JsonCodec},
    key::Key,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Item {
    id: u32,
    name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let front = LruByteCache::new(256);
    let tmp = tempfile::tempdir()?;
    let fs = FsBackend::new(tmp.path(), JsonCodec.file_extension())?;
    let back = BackendByteCache::new(fs);

    let rt = ReadThroughByteCache::new(front, back);
    let typed = SerdeCache::new(JsonCodec, rt);

    let k = Key::new("item:7")?;
    let v = Item {
        id: 7,
        name: "seven".into(),
    };

    <_ as Cache<Item>>::put(&typed, &k, &v)?;
    let out: Item = <_ as Cache<Item>>::get(&typed, &k)?.unwrap();
    assert_eq!(out, v);

    println!("cache_readthrough ok");
    Ok(())
}
