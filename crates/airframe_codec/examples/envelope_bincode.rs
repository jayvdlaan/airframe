use airframe_codec::codecs::BincodeCodec;
use airframe_codec::Envelope;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Greeting {
    who: String,
    n: u32,
}

fn main() {
    let codec = BincodeCodec;
    let value = Greeting {
        who: "Alice".into(),
        n: 7,
    };

    // Pack into an envelope tagged with codec name
    let env = Envelope::pack(&codec, &value).expect("pack");
    println!(
        "Envelope codec: {} (payload {} bytes)",
        env.codec,
        env.payload.len()
    );

    // Unpack and verify
    let unpacked: Greeting = env.unpack(&codec).expect("unpack");
    assert_eq!(unpacked, value);
    println!("Unpacked value matches: {:?}", unpacked);
}
