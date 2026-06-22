#[cfg(feature = "zstd")]
use airframe_compress::AirframeCompressError;
#[cfg(feature = "zstd")]
use airframe_compress::Compressor;

fn main() {
    #[cfg(feature = "zstd")]
    {
        let z = airframe_compress::Zstd::new(3);
        let input = b"This will be corrupted";
        let compressed = z.compress(input).expect("compress");

        // corrupt the buffer by truncating it
        let bad = &compressed[..compressed.len() / 2];
        match z.decompress(bad) {
            Ok(_) => println!("Unexpected success"),
            Err(AirframeCompressError::DecompressError(msg)) => {
                println!("Expected decompression error: {}", msg)
            }
            Err(e) => println!("Other error: {} (code={})", e, e.to_int()),
        }
    }
    #[cfg(not(feature = "zstd"))]
    {
        println!("Enable zstd feature to run this example");
    }
}
