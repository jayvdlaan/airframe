use airframe_codec::error::AirframeCodecError;
use airframe_codec::{content_id_sha256, Codec};
use airframe_core::error::ErrorRange;

#[test]
fn error_to_int_mappings() {
    let base = ErrorRange::Codec.base();
    assert_eq!(AirframeCodecError::CoreError("x".into()).to_int(), base);
    assert_eq!(
        AirframeCodecError::EncodeError("x".into()).to_int(),
        base + 1
    );
    assert_eq!(
        AirframeCodecError::DecodeError("x".into()).to_int(),
        base + 2
    );
    assert_eq!(
        AirframeCodecError::InvalidData("x".into()).to_int(),
        base + 3
    );
    assert_eq!(
        AirframeCodecError::Unsupported("x".into()).to_int(),
        base + 4
    );
}

#[test]
fn codec_content_id_is_deterministic_and_changes_with_input() {
    struct DummyCodec;
    impl Codec for DummyCodec {
        const NAME: &'static str = "dummy";
        fn encode<T: serde::Serialize>(&self, _t: &T) -> Result<Vec<u8>, AirframeCodecError> {
            Err(AirframeCodecError::Unsupported("encode not used".into()))
        }
        fn decode<T: serde::de::DeserializeOwned>(
            &self,
            _bytes: &[u8],
        ) -> Result<T, AirframeCodecError> {
            Err(AirframeCodecError::Unsupported("decode not used".into()))
        }
    }

    let c = DummyCodec;
    let a = b"same-bytes";
    let b = b"different-bytes";
    let cid1 = c.content_id(a);
    let cid2 = c.content_id(a);
    let cid3 = c.content_id(b);
    assert_eq!(cid1, cid2);
    assert_ne!(cid1, cid3);

    // Also ensure top-level helper matches method
    assert_eq!(cid1, content_id_sha256(a));
}
