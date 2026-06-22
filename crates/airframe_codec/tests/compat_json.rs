use airframe_codec::codecs::JsonCodec;
use airframe_codec::Codec;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Model {
    id: u32,
    name: String,
}

#[test]
fn decode_fixed_json_is_stable() {
    // This JSON payload is a fixed fixture that should continue to decode into the same struct.
    let bytes = br#"{"id":1,"name":"alpha"}"#;
    let json = JsonCodec;
    let out: Model = json.decode(bytes).unwrap();
    assert_eq!(
        out,
        Model {
            id: 1,
            name: "alpha".into()
        }
    );
}
