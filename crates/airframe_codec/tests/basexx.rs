use airframe_codec::basexx::*;
use base64::Engine; // bring trait into scope for STANDARD.encode

#[test]
fn base64_roundtrip_and_errors() {
    let data = b"hello world";
    let enc = base64_encode(data);
    assert_eq!(enc, base64::engine::general_purpose::STANDARD.encode(data));
    let dec = base64_decode(&enc).unwrap();
    assert_eq!(dec, data);

    // Invalid character should error
    assert!(base64_decode("@@@").is_err());
}

#[test]
fn base32_roundtrip_and_errors() {
    let data = b"airframe";
    let enc = base32_encode(data);
    assert_eq!(enc, data_encoding::BASE32_NOPAD.encode(data));
    let dec = base32_decode(&enc).unwrap();
    assert_eq!(dec, data);

    // Invalid character for BASE32_NOPAD
    assert!(base32_decode("!!!!").is_err());
    // Padding not allowed in NOPAD alphabet
    assert!(base32_decode("ME==").is_err());
}

#[test]
fn base16_roundtrip_and_errors() {
    let data = b"codec";
    let enc = base16_encode(data);
    assert_eq!(enc, data_encoding::HEXLOWER.encode(data));
    let dec = base16_decode(&enc).unwrap();
    assert_eq!(dec, data);

    // Non-hex chars
    assert!(base16_decode("xz").is_err());
    // Odd length should be invalid
    assert!(base16_decode("abc").is_err());
}

#[test]
fn multibase_prefixes_match_encodings() {
    let data = b"multibase";
    let m16 = multibase_encode(Multibase::Base16, data);
    let m32 = multibase_encode(Multibase::Base32, data);
    let m64 = multibase_encode(Multibase::Base64, data);

    assert!(m16.starts_with('f'));
    assert!(m32.starts_with('b'));
    assert!(m64.starts_with('m'));

    assert_eq!(&m16[1..], base16_encode(data));
    assert_eq!(&m32[1..], base32_encode(data));
    assert_eq!(&m64[1..], base64_encode(data));
}
