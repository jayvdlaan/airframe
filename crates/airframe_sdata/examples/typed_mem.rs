use airframe_data::{backend::mem::MemBackend, codec::JsonCodec};
use airframe_sdata::{
    model::DataModel,
    schema::{Migrator, SchemaRegistry},
    store::{KeySpace, TypedRepo},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
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
            obj.entry("age").or_insert(json!(0));
        }
        Ok(v)
    }
}

fn main() -> anyhow::Result<()> {
    // Register a single migration step 1 -> 2
    let mut reg = SchemaRegistry::new();
    reg.register_step("user", 1, Arc::new(UserMigV1toV2));
    let reg = Arc::new(reg);

    let backend = MemBackend::new();
    let codec = JsonCodec;
    let repo: TypedRepo<_, _, UserV2> = TypedRepo::new(backend, codec, reg.clone());

    let ks = KeySpace::new("users");
    let key = ks.key("alice").unwrap();
    let v = UserV2 {
        id: "alice".into(),
        name: "Alice".into(),
        age: 1,
    };
    repo.put(&key, &v)?;
    let got = repo.get(&key)?.unwrap();
    assert_eq!(got, v);
    println!("roundtrip ok: {:?}", got);
    Ok(())
}
