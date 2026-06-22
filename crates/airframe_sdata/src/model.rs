use serde::{de::DeserializeOwned, Serialize};

use crate::error::{AirframeSdataError, Result};

pub trait DataModel: Serialize + DeserializeOwned + Clone + Send + Sync + 'static {
    const SCHEMA_NAME: &'static str;
    const SCHEMA_VERSION: u32;
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl<T: DataModel> DataModel for Box<T> {
    const SCHEMA_NAME: &'static str = T::SCHEMA_NAME;
    const SCHEMA_VERSION: u32 = T::SCHEMA_VERSION;
    fn validate(&self) -> Result<()> {
        (**self).validate()
    }
}

pub fn ensure_schema<T: DataModel>(name: &str, version: u32) -> Result<()> {
    if name != T::SCHEMA_NAME {
        return Err(AirframeSdataError::MigrationError(format!(
            "schema mismatch: stored={}, expected={}",
            name,
            T::SCHEMA_NAME
        )));
    }
    if version > T::SCHEMA_VERSION {
        return Err(AirframeSdataError::MigrationError(format!(
            "cannot downgrade from newer version {} to {}",
            version,
            T::SCHEMA_VERSION
        )));
    }
    Ok(())
}
