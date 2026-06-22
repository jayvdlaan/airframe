use crate::error::{AirframeDataError, Result};
use serde::{de::DeserializeOwned, Serialize};

pub trait Codec: Clone + Send + Sync + 'static {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>>;
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T>;
    fn file_extension(&self) -> &'static str {
        "dat"
    }
}

#[derive(Debug, Clone, Default)]
pub struct JsonCodec;

impl Codec for JsonCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(value).map_err(|e| AirframeDataError::Codec(e.to_string()))
    }
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        serde_json::from_slice(bytes).map_err(|e| AirframeDataError::Codec(e.to_string()))
    }
    fn file_extension(&self) -> &'static str {
        "json"
    }
}

#[derive(Debug, Clone, Default)]
pub struct BincodeCodec;

impl Codec for BincodeCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        bincode::serialize(value).map_err(|e| AirframeDataError::Codec(e.to_string()))
    }
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
        bincode::deserialize(bytes).map_err(|e| AirframeDataError::Codec(e.to_string()))
    }
    fn file_extension(&self) -> &'static str {
        "bin"
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

    #[test]
    fn json_codec_roundtrip() {
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
    fn json_codec_file_extension() {
        let codec = JsonCodec;
        assert_eq!(codec.file_extension(), "json");
    }

    #[test]
    fn json_codec_decode_error() {
        let codec = JsonCodec;
        let result: Result<TestData> = codec.decode(b"invalid json");
        assert!(result.is_err());
    }

    #[test]
    fn bincode_codec_roundtrip() {
        let codec = BincodeCodec;
        let data = TestData {
            name: "bin".to_string(),
            value: 100,
        };

        let encoded = codec.encode(&data).expect("encode");
        let decoded: TestData = codec.decode(&encoded).expect("decode");

        assert_eq!(data, decoded);
    }

    #[test]
    fn bincode_codec_file_extension() {
        let codec = BincodeCodec;
        assert_eq!(codec.file_extension(), "bin");
    }

    #[test]
    fn bincode_codec_decode_error() {
        let codec = BincodeCodec;
        // Empty bytes cause decode error
        let result: Result<TestData> = codec.decode(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn codec_default_extension() {
        // Test the default implementation
        #[derive(Clone)]
        struct CustomCodec;
        impl Codec for CustomCodec {
            fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
                serde_json::to_vec(value).map_err(|e| AirframeDataError::Codec(e.to_string()))
            }
            fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T> {
                serde_json::from_slice(bytes).map_err(|e| AirframeDataError::Codec(e.to_string()))
            }
            // Uses default file_extension()
        }

        let codec = CustomCodec;
        assert_eq!(codec.file_extension(), "dat");
    }
}
