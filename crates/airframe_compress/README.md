# airframe_compress

Short description: Minimal compression abstraction with pluggable algorithms and streaming helpers.

## Overview

airframe_compress provides a small, feature-gated compression interface used across Airframe (e.g., cache layers in airframe_data, pdata pipelines). It offers whole-buffer compression/decompression via a `Compressor` trait and streaming helpers to compress while writing and decompress while reading.

## Logical pieces

- Compressor trait: `compress(&self, &[u8]) -> Result<Vec<u8>>`, `decompress(&self, &[u8]) -> Result<Vec<u8>>`, `name()`
- Algorithms (feature-gated): Zstd (default), LZ4, Gzip, Brotli
- stream: `new_compress_writer`, `new_decompress_reader` for streaming I/O
- Error type: `AirframeCompressError` with stable integer mapping

## Airframe module compatibility

- Compatibility: This crate does not export an Airframe module; it’s a utility library consumed by other crates.

## Dependencies

- Rust dependencies/features: see Cargo.toml
  - Features: `zstd` (default), `lz4`, `gzip`, `brotli` select backend algorithms
- System libraries: none by default; uses pure-Rust backends where possible
- Airframe capacities/modules: none

## Setup / Installation

```toml
[dependencies]
airframe_compress = { path = "../airframe_compress" }
```

Select algorithms either via Cargo features in your crate or when building:

```toml
# Example: enable only lz4
airframe_compress = { path = "../airframe_compress", default-features = false, features = ["lz4"] }
```

## Usage

### Example 1: Buffer API (zstd)

```rust
use airframe_compress::Compressor;

#[cfg(feature = "zstd")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let z = airframe_compress::Zstd::new(3);
    let input = b"hello compression! hello compression! hello compression!";
    let compressed = z.compress(input)?;
    let roundtrip = z.decompress(&compressed)?;
    assert_eq!(roundtrip, input);
    Ok(())
}

#[cfg(not(feature = "zstd"))]
fn main() { println!("Enable the 'zstd' feature to run this example"); }
```

### Example 2: Streaming API (zstd)

```rust
use std::io::{Write, Read};
use airframe_compress::Compressor;
use airframe_compress::stream::{new_compress_writer, new_decompress_reader};

#[cfg(feature = "zstd")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let algo = airframe_compress::Zstd::new(3);

    let input = b"repeated data...".repeat(1000);
    let mut w = new_compress_writer(&algo, Vec::new())?;
    w.write_all(&input)?;
    let compressed = w.into_inner()?;

    let cursor = std::io::Cursor::new(compressed);
    let mut r = new_decompress_reader(&algo, cursor)?;
    let mut out = Vec::new();
    r.read_to_end(&mut out)?;
    assert_eq!(out, input);
    Ok(())
}

#[cfg(not(feature = "zstd"))]
fn main() { println!("Enable the 'zstd' feature to run this example"); }
```

## Status

APIs implemented (compression algorithms and streaming helpers). No Airframe module interface.

## License

This project is licensed under the repository license; see the top-level LICENSE file.
