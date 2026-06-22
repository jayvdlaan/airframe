use airframe_data::backend::mem::MemBackend;
use airframe_data::cache::BackendByteCache;
use airframe_data::codec::JsonCodec;
use airframe_data::key::Key;
use airframe_sdata::cache::{SDataCacheBuilder, SchemaCache};
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
    // Bytes view over a mem backend
    let bytes = BackendByteCache::new(MemBackend::new());

    // Schema registry with a v1->v2 migration
    let mut reg = SchemaRegistry::new();
    reg.register_step("user", 1, Arc::new(UserMigV1toV2));
    let reg = Arc::new(reg);

    // Build the schema-aware cache
    let cache: SchemaCache<_, _, UserV2> = SDataCacheBuilder::new()
        .bytes(bytes)
        .registry(reg)
        .build_typed(JsonCodec)?;

    // Put and get a v2 value
    let k = Key::new("user:alice")?;
    let v2 = UserV2 {
        id: "alice".into(),
        name: "Alice".into(),
        age: 0,
    };
    cache.put(&k, &v2)?;
    let out = cache.get(&k)?.unwrap();
    assert_eq!(out, v2);

    println!("schema_cache_mem ok: {:?}", out);
    Ok(())
}
