fn main() {
    #[cfg(feature = "lz4")]
    {
        use airframe_compress::Compressor;
        let lz4 = airframe_compress::Lz4::new();
        let input = b"LZ4 is super fast! ".repeat(2000);
        let compressed = lz4.compress(&input).expect("compress");
        let roundtrip = lz4.decompress(&compressed).expect("decompress");
        println!(
            "{} -> ext .{} | in={} out={} ok={}",
            lz4.name(),
            lz4.default_extension(),
            input.len(),
            compressed.len(),
            input == roundtrip
        );
    }
    #[cfg(not(feature = "lz4"))]
    {
        println!("Enable lz4 feature to run this example");
    }
}
