// kept intentionally minimal; no imports required when printing only

fn main() {
    #[cfg(feature = "zstd")]
    {
        use airframe_compress::Compressor;
        let z = airframe_compress::Zstd::new(3);
        let input = b"hello compression! hello compression! hello compression!";
        let c = z.compress(input).expect("compress");
        let d = z.decompress(&c).expect("decompress");
        println!(
            "algo={} ext=.{}\ninput={} bytes, compressed={} bytes, ok={}",
            z.name(),
            z.default_extension(),
            input.len(),
            c.len(),
            input.as_slice() == d.as_slice()
        );
    }
    #[cfg(not(feature = "zstd"))]
    {
        println!("zstd feature not enabled; enable with --features zstd");
    }
}
