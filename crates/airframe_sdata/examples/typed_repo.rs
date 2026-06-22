use airframe_data::backend::mem::MemBackend;
use airframe_data::backend::KvBackend;
use airframe_data::codec::{Codec, JsonCodec};
use airframe_sdata::model::DataModel;
use airframe_sdata::schema::{Migrator, SchemaRegistry};
use airframe_sdata::store::{KeySpace, TypedRepo};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct UserV2 {
    id: String,
    name: String,
    age: u32,
}

impl DataModel for UserV2 {
    const SCHEMA_NAME: &'static str = "user";
    const SCHEMA_VERSION: u32 = 2;
    fn validate(&self) -> airframe_sdata::error::Result<()> {
        if self.name.is_empty() {
            return Err(airframe_sdata::error::AirframeSdataError::ValidationError(
                "name empty".into(),
            ));
        }
        Ok(())
    }
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
        // v1 had fields { id, name } only; create age=0
        if let Some(obj) = v.as_object_mut() {
            obj.entry("age").or_insert(json!(0));
        }
        Ok(v)
    }
}

fn main() {
    let backend = MemBackend::new();
    let codec = JsonCodec;
    let mut reg = SchemaRegistry::new();
    reg.register_step("user", 1, Arc::new(UserMigV1toV2));
    let reg = Arc::new(reg);

    let repo: TypedRepo<_, _, UserV2> = TypedRepo::new(backend.clone(), codec.clone(), reg.clone());
    let ks = KeySpace::new("users");
    let key = ks.key("alice").unwrap();

    // Simulate legacy v1 record
    #[derive(Serialize)]
    struct Legacy {
        id: String,
        name: String,
    }
    let legacy = Legacy {
        id: "alice".into(),
        name: "Alice".into(),
    };
    #[derive(Serialize)]
    struct Env<'a, T> {
        schema: &'a str,
        version: u32,
        data: &'a T,
    }
    let env = Env {
        schema: "user",
        version: 1,
        data: &legacy,
    };
    let bytes = codec.encode(&env).unwrap();
    backend.put_bytes(&key, &bytes).unwrap();

    let value = repo.get(&key).unwrap().unwrap();
    assert_eq!(
        value,
        UserV2 {
            id: "alice".into(),
            name: "Alice".into(),
            age: 0
        }
    );
    println!("Migrated and validated value: {:?}", value);

    // Now write back a v2 value
    repo.put(&key, &value).unwrap();
    let loaded = repo.get(&key).unwrap().unwrap();
    assert_eq!(loaded, value);
}
