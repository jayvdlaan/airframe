use airframe_data::backend::fs::FsBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::cache::{LruByteCache, ReadThroughByteCache};
use airframe_data::codec::{Codec, JsonCodec};
use airframe_data::key::Key;
use airframe_sdata::cache::SchemaCache;
use airframe_sdata::model::DataModel;
use airframe_sdata::schema::{Migrator, SchemaRegistry};
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct UserV2 {
    id: String,
    name: String,
    age: u32,
}
impl DataModel for UserV2 {
    const SCHEMA_NAME: &'static str = "user";
    const SCHEMA_VERSION: u32 = 2;
}

struct UserMigV1toV2;
impl Migrator for UserMigV1toV2 {
    fn schema_name(&self) -> &'static str {
        "user"
    }
    fn migrate(
        &self,
        _from: u32,
        _to: u32,
        mut v: serde_json::Value,
    ) -> airframe_sdata::error::Result<serde_json::Value> {
        if let Some(obj) = v.as_object_mut() {
            obj.entry("age").or_insert(serde_json::json!(0));
        }
        Ok(v)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Back: filesystem bytes via BackendByteCache
    let tmp = tempfile::tempdir()?;
    let codec_inst = JsonCodec;
    let fs = FsBackend::new(tmp.path(), codec_inst.file_extension())?;
    let back_bytes = BackendByteCache::new(fs);

    // Front: small LRU
    let front = LruByteCache::new(64);

    // Compose read-through
    let two_level = ReadThroughByteCache::new(front, back_bytes);

    // Schema registry
    let mut reg = SchemaRegistry::new();
    reg.register_step("user", 1, Arc::new(UserMigV1toV2));
    let reg = Arc::new(reg);

    // Typed cache over the two-level bytes cache
    let cache: SchemaCache<_, _, UserV2> = SchemaCache::new(JsonCodec, two_level, reg);

    let k = Key::new("user:bob")?;
    let v = UserV2 {
        id: "bob".into(),
        name: "Bob".into(),
        age: 33,
    };
    cache.put(&k, &v)?;
    let out = cache.get(&k)?.unwrap();
    assert_eq!(out, v);

    println!("schema_cache_readthrough ok: {:?}", out);
    Ok(())
}
