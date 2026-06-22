use airframe_codec::codecs::der;
use airframe_codec::content_id_sha256;
use airframe_codec::AirframeCodecError;

#[test]
fn der_roundtrip_with_octet_string() {
    // Use a built-in rasn type that implements AsnType + Encode/Decode
    use rasn::types::OctetString;
    let value = OctetString::from(vec![1u8, 2, 3, 4, 5]);
    let bytes = der::encode(&value).expect("der encode");
    let out: OctetString = der::decode(&bytes).expect("der decode");
    assert_eq!(out, value);
}

#[test]
fn der_decode_error_on_corrupted_bytes() {
    use rasn::types::OctetString;
    let value = OctetString::from(vec![9u8, 8, 7]);
    let mut bytes = der::encode(&value).expect("encode");
    // Corrupt the length/tag to trigger a decode failure
    if !bytes.is_empty() {
        bytes[0] = 0xFF;
    }
    let err = der::decode::<OctetString>(&bytes).unwrap_err();
    match err {
        AirframeCodecError::DecodeError(msg) => assert!(msg.to_lowercase().contains("der")),
        other => panic!("expected DecodeError, got {other:?}"),
    }
}

#[test]
fn content_id_empty_and_large_inputs() {
    // Empty input should be valid and stable
    let cid_empty_1 = content_id_sha256(&[]);
    let cid_empty_2 = content_id_sha256(&[]);
    assert_eq!(cid_empty_1, cid_empty_2);
    assert_eq!(cid_empty_1.as_bytes().len(), 32); // sha256 length

    // Large input (couple of KB) should compute and be deterministic
    let large: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
    let cid_large_1 = content_id_sha256(&large);
    let cid_large_2 = content_id_sha256(&large);
    assert_eq!(cid_large_1, cid_large_2);

    // Hex formatting should match byte length * 2
    let hex = cid_large_1.to_hex();
    assert_eq!(hex.len(), cid_large_1.as_bytes().len() * 2);
}
