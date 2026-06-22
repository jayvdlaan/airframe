pub mod basexx;
pub mod codecs;
pub mod content_id;
pub mod envelope;
pub mod error;
#[cfg(feature = "module")]
pub mod module;

pub use content_id::{content_id_sha256, ContentId};
pub use envelope::Envelope;
pub use error::AirframeCodecError;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Core abstraction over a codec implementation.
pub trait Codec {
    const NAME: &'static str;
    fn encode<T: Serialize>(&self, t: &T) -> Result<Vec<u8>, AirframeCodecError>;
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, AirframeCodecError>;
    fn content_id(&self, bytes: &[u8]) -> ContentId {
        content_id_sha256(bytes)
    }
}
