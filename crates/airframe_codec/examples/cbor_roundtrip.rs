use airframe_codec::codecs::CborCodec;
use airframe_codec::Codec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let codec = CborCodec;
    let p = Point { x: 10, y: -5 };

    let bytes = codec.encode(&p).expect("encode cbor");
    println!("CBOR bytes: {}", bytes.len());

    let p2: Point = codec.decode(&bytes).expect("decode cbor");
    assert_eq!(p, p2);
    println!("Roundtrip success: {:?}", p2);
}
