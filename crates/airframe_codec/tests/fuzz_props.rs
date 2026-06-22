use airframe_codec::codecs::{BincodeCodec, CborCodec, JsonCodec};
use airframe_codec::Codec;
use proptest::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Small {
    n: u8,
    s: String,
}

// Generate small ASCII strings (avoid very large or exotic unicode to keep CI fast and deterministic)
fn small_ascii() -> impl Strategy<Value = String> {
    // 0..16 chars, visible ASCII subset
    prop::collection::vec(prop::char::range(' ', '~'), 0..16).prop_map(|v| v.into_iter().collect())
}

proptest! {
    #[test]
    fn roundtrip_bincode_prop(n in any::<u8>(), s in small_ascii()) {
        let value = Small { n, s };
        let bin = BincodeCodec;
        let bytes = bin.encode(&value).unwrap();
        let out: Small = bin.decode(&bytes).unwrap();
        prop_assert_eq!(out, value);
    }

    #[test]
    fn roundtrip_cbor_prop(n in any::<u8>(), s in small_ascii()) {
        let value = Small { n, s };
        let cbor = CborCodec;
        let bytes = cbor.encode(&value).unwrap();
        let out: Small = cbor.decode(&bytes).unwrap();
        prop_assert_eq!(out, value);
    }

    #[test]
    fn roundtrip_json_prop(n in any::<u8>(), s in small_ascii()) {
        let value = Small { n, s };
        let json = JsonCodec;
        let bytes = json.encode(&value).unwrap();
        let out: Small = json.decode(&bytes).unwrap();
        prop_assert_eq!(out, value);
    }
}
