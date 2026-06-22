use airframe_core::error::{AirframeError, ErrorRange};

fn main() {
    // Convert to numeric code and back
    let e = AirframeError::InvalidState;
    let code = e.to_int();
    println!(
        "InvalidState -> code {} (in Core range: {}..={})",
        code,
        ErrorRange::Core.base(),
        ErrorRange::Core.max()
    );

    let roundtrip = AirframeError::from_int(code).unwrap();
    println!("Roundtrip: {} -> {:?}", code, roundtrip);

    // Unknown code stays as Other
    if let Some(AirframeError::Other(c)) = AirframeError::from_int(4242) {
        println!("Unknown code remains Other({})", c);
    }
}
