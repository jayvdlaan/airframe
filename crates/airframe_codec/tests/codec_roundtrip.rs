use airframe_codec::codecs::{BincodeCodec, CborCodec, JsonCodec};
use airframe_codec::content_id_sha256;
use airframe_codec::Codec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Sample {
    a: u32,
    b: String,
}

#[test]
fn roundtrip_cbor() {
    let value = Sample {
        a: 42,
        b: "hello".into(),
    };
    let cbor = CborCodec;
    let bytes = cbor.encode(&value).unwrap();
    let cid1 = content_id_sha256(&bytes);
    let val2: Sample = cbor.decode(&bytes).unwrap();
    assert_eq!(value, val2);
    let cid2 = content_id_sha256(&bytes);
    assert_eq!(cid1, cid2);
}

#[test]
fn roundtrip_bincode() {
    let value = Sample {
        a: 7,
        b: "world".into(),
    };
    let bin = BincodeCodec;
    let bytes = bin.encode(&value).unwrap();
    let cid1 = content_id_sha256(&bytes);
    let val2: Sample = bin.decode(&bytes).unwrap();
    assert_eq!(value, val2);
    let cid2 = content_id_sha256(&bytes);
    assert_eq!(cid1, cid2);
}

#[test]
fn roundtrip_json() {
    let value = Sample {
        a: 9,
        b: "json".into(),
    };
    let json = JsonCodec;
    let bytes = json.encode(&value).unwrap();
    let cid1 = content_id_sha256(&bytes);
    let val2: Sample = json.decode(&bytes).unwrap();
    assert_eq!(value, val2);
    let cid2 = content_id_sha256(&bytes);
    assert_eq!(cid1, cid2);
}
