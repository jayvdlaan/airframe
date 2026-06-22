use crate::error::AirframeCodecError;
use crate::Codec;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tracing::{debug, error, trace};

/// JSON codec based on serde_json.
pub struct JsonCodec;
impl Codec for JsonCodec {
    const NAME: &'static str = "json";

    #[tracing::instrument(level = "trace", skip(self, t))]
    fn encode<T: Serialize>(&self, t: &T) -> Result<Vec<u8>, AirframeCodecError> {
        trace!(target = "airframe_codec", "encode start");
        match serde_json::to_vec(t) {
            Ok(out) => {
                debug!(target = "airframe_codec", out_len = out.len(), "encode ok");
                Ok(out)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "encode failed");
                Err(AirframeCodecError::EncodeError(format!("JSON encode: {e}")))
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(self, bytes))]
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, AirframeCodecError> {
        trace!(target = "airframe_codec", len = bytes.len(), "decode start");
        match serde_json::from_slice(bytes) {
            Ok(v) => {
                debug!(target = "airframe_codec", "decode ok");
                Ok(v)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "decode failed");
                Err(AirframeCodecError::DecodeError(format!("JSON decode: {e}")))
            }
        }
    }
}

pub struct CborCodec;
impl Codec for CborCodec {
    const NAME: &'static str = "cbor";

    #[tracing::instrument(level = "trace", skip(self, t))]
    fn encode<T: Serialize>(&self, t: &T) -> Result<Vec<u8>, AirframeCodecError> {
        trace!(target = "airframe_codec", "encode start");
        match serde_cbor::to_vec(t) {
            Ok(out) => {
                debug!(target = "airframe_codec", out_len = out.len(), "encode ok");
                Ok(out)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "encode failed");
                Err(AirframeCodecError::EncodeError(format!("CBOR encode: {e}")))
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(self, bytes))]
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, AirframeCodecError> {
        trace!(target = "airframe_codec", len = bytes.len(), "decode start");
        match serde_cbor::from_slice(bytes) {
            Ok(v) => {
                debug!(target = "airframe_codec", "decode ok");
                Ok(v)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "decode failed");
                Err(AirframeCodecError::DecodeError(format!("CBOR decode: {e}")))
            }
        }
    }
}

pub struct BincodeCodec;
impl Codec for BincodeCodec {
    const NAME: &'static str = "bincode";

    #[tracing::instrument(level = "trace", skip(self, t))]
    fn encode<T: Serialize>(&self, t: &T) -> Result<Vec<u8>, AirframeCodecError> {
        trace!(target = "airframe_codec", "encode start");
        match bincode::serialize(t) {
            Ok(out) => {
                debug!(target = "airframe_codec", out_len = out.len(), "encode ok");
                Ok(out)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "encode failed");
                Err(AirframeCodecError::EncodeError(format!(
                    "Bincode encode: {e}"
                )))
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(self, bytes))]
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, AirframeCodecError> {
        trace!(target = "airframe_codec", len = bytes.len(), "decode start");
        match bincode::deserialize(bytes) {
            Ok(v) => {
                debug!(target = "airframe_codec", "decode ok");
                Ok(v)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "decode failed");
                Err(AirframeCodecError::DecodeError(format!(
                    "Bincode decode: {e}"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    // JSON codec tests
    #[test]
    fn json_encode_decode_roundtrip() {
        let codec = JsonCodec;
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let encoded = codec.encode(&data).expect("encode");
        let decoded: TestData = codec.decode(&encoded).expect("decode");

        assert_eq!(data, decoded);
    }

    #[test]
    fn json_decode_invalid_returns_error() {
        let codec = JsonCodec;
        let invalid = b"not valid json {";

        let result: Result<TestData, _> = codec.decode(invalid);
        assert!(result.is_err());
    }

    // CBOR codec tests
    #[test]
    fn cbor_encode_decode_roundtrip() {
        let codec = CborCodec;
        let data = TestData {
            name: "cbor".to_string(),
            value: 100,
        };

        let encoded = codec.encode(&data).expect("encode");
        let decoded: TestData = codec.decode(&encoded).expect("decode");

        assert_eq!(data, decoded);
    }

    #[test]
    fn cbor_decode_invalid_returns_error() {
        let codec = CborCodec;
        let invalid = b"not valid cbor";

        let result: Result<TestData, _> = codec.decode(invalid);
        assert!(result.is_err());
    }

    // Bincode codec tests
    #[test]
    fn bincode_encode_decode_roundtrip() {
        let codec = BincodeCodec;
        let data = TestData {
            name: "bincode".to_string(),
            value: 200,
        };

        let encoded = codec.encode(&data).expect("encode");
        let decoded: TestData = codec.decode(&encoded).expect("decode");

        assert_eq!(data, decoded);
    }

    #[test]
    fn bincode_decode_invalid_returns_error() {
        let codec = BincodeCodec;
        // Bincode is binary format, truncated data causes error
        let invalid = &[0u8, 1, 2];

        let result: Result<TestData, _> = codec.decode(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn codec_names_are_correct() {
        assert_eq!(JsonCodec::NAME, "json");
        assert_eq!(CborCodec::NAME, "cbor");
        assert_eq!(BincodeCodec::NAME, "bincode");
    }
}

// ASN.1 DER helpers via rasn for types that implement rasn traits.
pub mod der {
    use super::*;
    use rasn::AsnType;

    #[tracing::instrument(level = "trace", skip(value))]
    pub fn encode<T: AsnType + rasn::Encode>(value: &T) -> Result<Vec<u8>, AirframeCodecError> {
        trace!(target = "airframe_codec", "encode start");
        match rasn::der::encode(value) {
            Ok(out) => {
                debug!(target = "airframe_codec", out_len = out.len(), "encode ok");
                Ok(out)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "encode failed");
                Err(AirframeCodecError::EncodeError(format!("DER encode: {e}")))
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(bytes))]
    pub fn decode<T: AsnType + rasn::Decode>(bytes: &[u8]) -> Result<T, AirframeCodecError> {
        trace!(target = "airframe_codec", len = bytes.len(), "decode start");
        match rasn::der::decode(bytes) {
            Ok(v) => {
                debug!(target = "airframe_codec", "decode ok");
                Ok(v)
            }
            Err(e) => {
                error!(target = "airframe_codec", error = ?e, "decode failed");
                Err(AirframeCodecError::DecodeError(format!("DER decode: {e}")))
            }
        }
    }
}
