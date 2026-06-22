fn main() {
    #[cfg(feature = "brotli")]
    {
        use airframe_compress::Compressor;
        let br = airframe_compress::Brotli::new(5);
        let input = b"Brotli can be great for text; this is some repeated text. ".repeat(1000);
        let compressed = br.compress(&input).expect("compress");
        let roundtrip = br.decompress(&compressed).expect("decompress");
        println!(
            "{}(quality={:?}) -> ext .{} | in={} out={} ok={}",
            br.name(),
            br.level(),
            br.default_extension(),
            input.len(),
            compressed.len(),
            input == roundtrip
        );
    }
    #[cfg(not(feature = "brotli"))]
    {
        println!("Enable brotli feature to run this example");
    }
}
