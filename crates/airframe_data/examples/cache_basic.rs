use airframe_data::{
    backend::mem::MemBackend,
    cache::{BackendByteCache, Cache, SerdeCache},
    codec::JsonCodec,
    key::Key,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Item {
    id: u32,
    name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = BackendByteCache::new(MemBackend::new());
    let typed = SerdeCache::new(JsonCodec, bytes);

    let k = Key::new("item:1")?;
    let v = Item {
        id: 1,
        name: "one".into(),
    };

    <SerdeCache<_, _> as Cache<Item>>::put(&typed, &k, &v)?;
    let got: Item = <SerdeCache<_, _> as Cache<Item>>::get(&typed, &k)?.unwrap();
    assert_eq!(got, v);

    println!("cache_basic ok");
    Ok(())
}
