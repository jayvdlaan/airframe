use airframe_codec::codecs::der;
use rasn::{AsnType, Decode, Encode};

#[derive(Debug, Clone, AsnType, Encode, Decode, PartialEq)]
pub struct Person {
    pub name: String,
    pub age: u8,
}

fn main() {
    let p = Person {
        name: "Alice".into(),
        age: 30,
    };
    let der_bytes = der::encode(&p).expect("der encode");
    println!("DER-encoded person: {} bytes", der_bytes.len());
    let decoded: Person = der::decode(&der_bytes).expect("der decode");
    assert_eq!(decoded, p);
    println!("DER roundtrip OK: {:?}", decoded);
}
