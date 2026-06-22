#[cfg(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
))]
use airframe_compress::{AirframeCompressError, Compressor};

#[cfg(any(
    feature = "zstd",
    feature = "lz4",
    feature = "gzip",
    feature = "brotli"
))]
fn data() -> Vec<u8> {
    // deterministic but compressible
    let mut v = Vec::new();
    for i in 0..10_000u32 {
        v.extend_from_slice(format!("line {:06}: hello world!\n", i).as_bytes());
    }
    v
}

#[cfg(feature = "zstd")]
#[test]
fn zstd_roundtrip_and_corruption() {
    let algo = airframe_compress::Zstd::new(3);
    let input = data();
    let compressed = algo.compress(&input).unwrap();
    let decompressed = algo.decompress(&compressed).unwrap();
    assert_eq!(decompressed, input);

    // Corrupt by truncating
    let bad = &compressed[..compressed.len() / 2];
    let err = algo.decompress(bad).unwrap_err();
    match err {
        AirframeCompressError::DecompressError(_) => {}
        _ => panic!("unexpected error type"),
    }
}

#[cfg(feature = "lz4")]
#[test]
fn lz4_roundtrip_and_corruption() {
    let algo = airframe_compress::Lz4::new();
    let input = data();
    let compressed = algo.compress(&input).unwrap();
    let decompressed = algo.decompress(&compressed).unwrap();
    assert_eq!(decompressed, input);

    // Try two corruption strategies and require at least one to be detected:
    // 1) Flip a byte in the middle
    let mut bad_flip = compressed.clone();
    if !bad_flip.is_empty() {
        let mid = bad_flip.len() / 2;
        bad_flip[mid] ^= 0xFF;
    }
    let flip_ok = match algo.decompress(&bad_flip) {
        Err(AirframeCompressError::DecompressError(_)) => true,
        Ok(out) => out != input,
        Err(_) => true,
    };

    // 2) Truncate the buffer
    let trunc = &compressed[..compressed.len() / 2];
    let trunc_ok = matches!(
        algo.decompress(trunc),
        Err(AirframeCompressError::DecompressError(_)) | Err(_)
    );

    assert!(
        flip_ok || trunc_ok,
        "LZ4 corruption was not detected by flip nor truncation"
    );
}

#[cfg(feature = "gzip")]
#[test]
fn gzip_roundtrip_and_corruption() {
    let algo = airframe_compress::Gzip::new(6);
    let input = data();
    let compressed = algo.compress(&input).unwrap();
    let decompressed = algo.decompress(&compressed).unwrap();
    assert_eq!(decompressed, input);

    let bad = &compressed[..compressed.len() / 2];
    let err = algo.decompress(bad).unwrap_err();
    match err {
        AirframeCompressError::DecompressError(_) => {}
        _ => panic!("unexpected error type"),
    }
}

#[cfg(feature = "brotli")]
#[test]
fn brotli_roundtrip_and_corruption() {
    let algo = airframe_compress::Brotli::new(5);
    let input = data();
    let compressed = algo.compress(&input).unwrap();
    let decompressed = algo.decompress(&compressed).unwrap();
    assert_eq!(decompressed, input);

    let bad = &compressed[..compressed.len() / 2];
    let err = algo.decompress(bad).unwrap_err();
    match err {
        AirframeCompressError::DecompressError(_) => {}
        _ => panic!("unexpected error type"),
    }
}

#[cfg(feature = "zstd")]
#[test]
fn zstd_stream_roundtrip() {
    use airframe_compress::stream::{new_compress_writer, new_decompress_reader};
    use std::io::{Read, Write};
    let algo = airframe_compress::Zstd::new(3);
    let input = data();

    // Compress via writer
    let mut w = new_compress_writer(&algo, Vec::new()).unwrap();
    w.write_all(&input).unwrap();
    let buf = w.into_inner().unwrap();

    // Decompress via reader
    let cursor = std::io::Cursor::new(buf);
    let mut r = new_decompress_reader(&algo, cursor).unwrap();
    let mut out = Vec::new();
    r.read_to_end(&mut out).unwrap();
    assert_eq!(out, input);
}
