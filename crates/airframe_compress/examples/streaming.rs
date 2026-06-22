#[cfg(feature = "zstd")]
use airframe_compress::stream::{new_compress_writer, new_decompress_reader};
#[cfg(feature = "zstd")]
use airframe_compress::Compressor;
#[cfg(feature = "zstd")]
use std::io::{Read, Write};

fn main() {
    #[cfg(feature = "zstd")]
    {
        let algo = airframe_compress::Zstd::new(5);
        let input: Vec<u8> = (0..200_000)
            .flat_map(|i| format!("line {:06}: lorem ipsum dolor sit amet; ", i).into_bytes())
            .collect();

        // Compress via writer
        let mut writer = new_compress_writer(&algo, Vec::new()).expect("new writer");
        writer.write_all(&input).expect("write");
        let compressed = writer.into_inner().expect("finish");

        // Decompress via reader
        let cursor = std::io::Cursor::new(compressed);
        let mut reader = new_decompress_reader(&algo, cursor).expect("new reader");
        let mut out = Vec::new();
        reader.read_to_end(&mut out).expect("read");
        println!(
            "streaming {}: in={} out={} eq={}",
            algo.name(),
            input.len(),
            out.len(),
            input == out
        );
    }
    #[cfg(not(feature = "zstd"))]
    {
        println!("Enable zstd feature to run this example");
    }
}
