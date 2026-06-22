use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{AirframeSdataError, Result};

pub trait Migrator: Send + Sync + 'static {
    fn schema_name(&self) -> &'static str;
    fn can_migrate(&self, from: u32, to: u32) -> bool {
        to == from + 1
    }
    fn migrate(&self, from_version: u32, to_version: u32, value: Value) -> Result<Value>;
}

#[derive(Default, Clone)]
pub struct SchemaRegistry {
    // schema_name -> (version -> migrator from version to version+1)
    #[allow(clippy::type_complexity)]
    inner: Arc<BTreeMap<&'static str, BTreeMap<u32, Arc<dyn Migrator>>>>,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::default(),
        }
    }

    pub fn register_step(
        &mut self,
        schema: &'static str,
        from_version: u32,
        migrator: Arc<dyn Migrator>,
    ) {
        let map = Arc::make_mut(&mut self.inner);
        let entry = map.entry(schema).or_default();
        entry.insert(from_version, migrator);
    }

    pub fn migrate_chain(
        &self,
        schema: &str,
        from: u32,
        to: u32,
        mut value: Value,
    ) -> Result<Value> {
        if from == to {
            return Ok(value);
        }
        if from > to {
            return Err(AirframeSdataError::MigrationError(format!(
                "downgrade not supported: {} -> {}",
                from, to
            )));
        }
        let map = self.inner.get(schema).ok_or_else(|| {
            AirframeSdataError::MigrationError(format!("no registry for schema {}", schema))
        })?;
        let mut v = from;
        while v < to {
            let mig = map.get(&v).ok_or_else(|| {
                AirframeSdataError::MigrationError(format!(
                    "no migrator {} v{}->v{}",
                    schema,
                    v,
                    v + 1
                ))
            })?;
            if !mig.can_migrate(v, v + 1) {
                return Err(AirframeSdataError::MigrationError(format!(
                    "migrator refused {} v{}->v{}",
                    schema,
                    v,
                    v + 1
                )));
            }
            value = mig.migrate(v, v + 1, value)?;
            v += 1;
        }
        Ok(value)
    }
}
