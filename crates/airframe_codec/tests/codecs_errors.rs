use airframe_codec::codecs::{BincodeCodec, CborCodec, JsonCodec};
use airframe_codec::AirframeCodecError;
use airframe_codec::Codec; // bring trait into scope for encode/decode

#[test]
fn json_decode_invalid_bytes_is_decode_error() {
    let json = JsonCodec;
    let bad = b"not-json";
    let err = json.decode::<serde_json::Value>(bad).unwrap_err();
    match err {
        AirframeCodecError::DecodeError(msg) => assert!(msg.to_lowercase().contains("json")),
        other => panic!("expected DecodeError, got {other:?}"),
    }
}

// Note: serde_json::to_vec rarely fails on encoding typical Rust types; we focus on decode errors.

#[test]
fn cbor_decode_invalid_is_decode_error() {
    let cbor = CborCodec;
    let bad = b"\xFF\xFF\xFF"; // invalid CBOR
    let err = cbor.decode::<serde_json::Value>(bad).unwrap_err();
    match err {
        AirframeCodecError::DecodeError(msg) => assert!(msg.to_lowercase().contains("cbor")),
        other => panic!("expected DecodeError, got {other:?}"),
    }
}

#[test]
fn bincode_decode_invalid_is_decode_error() {
    let bin = BincodeCodec;
    let bad = b"\x00\x01\x02\x03"; // garbage
    let err = bin.decode::<u64>(bad).unwrap_err();
    match err {
        AirframeCodecError::DecodeError(msg) => assert!(msg.to_lowercase().contains("bincode")),
        other => panic!("expected DecodeError, got {other:?}"),
    }
}
