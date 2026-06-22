#[cfg(feature = "zstd")]
use airframe_compress::Compressor;

fn main() {
    #[cfg(feature = "zstd")]
    {
        let z = airframe_compress::Zstd::new(3);
        let input = b"hello compression! hello compression! hello compression!";
        let compressed = z.compress(input).expect("compress");
        let roundtrip = z.decompress(&compressed).expect("decompress");
        println!(
            "{}(level={:?}) -> ext .{} | in={} out={} ok={}",
            z.name(),
            z.level(),
            z.default_extension(),
            input.len(),
            compressed.len(),
            input == roundtrip.as_slice()
        );
    }
    #[cfg(not(feature = "zstd"))]
    {
        println!("Enable zstd feature to run this example");
    }
}
