use airframe_codec::codecs::{CborCodec, JsonCodec};
use airframe_codec::{AirframeCodecError, Codec, Envelope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct S {
    x: u32,
    y: String,
}

#[test]
fn envelope_pack_and_unpack_success() {
    let v = S {
        x: 5,
        y: "ok".into(),
    };
    let json = JsonCodec;
    let env = Envelope::pack(&json, &v).expect("pack");
    assert_eq!(env.codec, JsonCodec::NAME);
    let out: S = env.unpack(&json).expect("unpack");
    assert_eq!(out, v);
}

#[test]
fn envelope_unpack_codec_mismatch_error() {
    let v = S {
        x: 10,
        y: "mismatch".into(),
    };
    let json = JsonCodec;
    let cbor = CborCodec;
    let env = Envelope::pack(&json, &v).expect("pack");
    let err = env.unpack::<CborCodec, S>(&cbor).unwrap_err();
    match err {
        AirframeCodecError::InvalidData(msg) => {
            assert!(msg.contains("does not match"), "unexpected msg: {msg}");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}
