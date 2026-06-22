use airframe_compress::AirframeCompressError;

#[test]
fn smoke_display_and_code() {
    let e = AirframeCompressError::CompressError("oops".into());
    assert!(format!("{}", e).contains("Compression error"));
    // Just ensure it maps to some u32 without panicking
    let _code = e.to_int();
}
