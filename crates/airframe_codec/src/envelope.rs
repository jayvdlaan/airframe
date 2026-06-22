use crate::error::AirframeCodecError;
use crate::Codec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope {
    pub codec: &'static str,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

impl Envelope {
    pub fn pack<C: Codec, T: Serialize>(codec: &C, value: &T) -> Result<Self, AirframeCodecError> {
        let payload = codec.encode(value)?;
        Ok(Envelope {
            codec: C::NAME,
            payload,
        })
    }

    pub fn unpack<C: Codec, T: for<'de> Deserialize<'de>>(
        &self,
        _codec: &C,
    ) -> Result<T, AirframeCodecError> {
        // Optionally validate codec name match
        if self.codec != C::NAME {
            return Err(AirframeCodecError::InvalidData(format!(
                "Envelope codec '{}' does not match expected '{}'",
                self.codec,
                C::NAME
            )));
        }
        _codec.decode::<T>(&self.payload)
    }
}
