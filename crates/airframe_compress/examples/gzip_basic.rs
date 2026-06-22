fn main() {
    #[cfg(feature = "gzip")]
    {
        use airframe_compress::Compressor;
        let gz = airframe_compress::Gzip::new(6);
        let input = b"Gzip for compatibility with many tools. ".repeat(1000);
        let compressed = gz.compress(&input).expect("compress");
        let roundtrip = gz.decompress(&compressed).expect("decompress");
        println!(
            "{}(level={:?}) -> ext .{} | in={} out={} ok={}",
            gz.name(),
            gz.level(),
            gz.default_extension(),
            input.len(),
            compressed.len(),
            input == roundtrip
        );
    }
    #[cfg(not(feature = "gzip"))]
    {
        println!("Enable gzip feature to run this example");
    }
}
