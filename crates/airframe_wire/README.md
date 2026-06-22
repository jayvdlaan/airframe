# airframe_wire

Bit-level binary protocol primitives for Airframe.

## Overview

airframe_wire provides a compact, MSB-first bit-level (de)serialization toolkit built around two types:

- `BitWriter` — packs values into a `Vec<u8>` at the bit level, allowing sub-byte fields to be written back-to-back without byte alignment.
- `BitReader<'a>` — unpacks values from a borrowed `&[u8]` in the same order, returning `Result<_, WireError>` and tracking buffer bounds.

Both sides support single bits (`write_bit`/`read_bit`), arbitrary bit widths up to 64 (`write_bits`/`read_bits`), the fixed-width integers `u8`/`u16`/`u32`/`u64` and `i8`/`i16`/`i32`, `f32` (encoded via its IEEE-754 bit pattern), `bool`, length-prefixed byte slices and UTF-8 strings (`write_bytes`/`write_string` and their readers, prefixed with a `u16` length), and LEB128-style variable-length `u32` values (`write_var_u32`/`read_var_u32`, 7-bit groups with a high continuation bit).

Reads fail with `WireError` rather than panicking: `WireError::BufferUnderflow { needed, available }` when there are not enough bits left, and `WireError::DecodeError(String)` for invalid UTF-8 or varint overflow. The reader also exposes `bit_position()` and `remaining_bits()`; the writer exposes `bit_position()`.

## Airframe module compatibility

This crate is a standalone primitive. It has zero internal (Airframe) dependencies, declares no Airframe module, and provides no capabilities. It is a plain library usable anywhere; nothing in the Airframe module system is required to use it.

## Dependencies

- Rust dependencies: none — uses only `std`.
- System libraries: none (pure Rust).
- Airframe capabilities/modules: none.

## Usage

```rust
use airframe_wire::{BitReader, BitWriter, WireError};

fn main() -> Result<(), WireError> {
    // Pack a mix of sub-byte fields and typed values.
    let mut w = BitWriter::new();
    w.write_bits(0b101, 3);            // 3-bit field
    w.write_bool(true);                // single bit
    w.write_u16(1000);
    w.write_var_u32(16_384);           // variable-length integer
    w.write_string("hello");           // u16 length prefix + UTF-8 bytes
    w.write_f32(std::f32::consts::PI);

    let bytes = w.into_bytes();

    // Unpack in the same order.
    let mut r = BitReader::new(&bytes);
    assert_eq!(r.read_bits(3)?, 0b101);
    assert!(r.read_bool()?);
    assert_eq!(r.read_u16()?, 1000);
    assert_eq!(r.read_var_u32()?, 16_384);
    assert_eq!(r.read_string()?, "hello");
    assert_eq!(r.read_f32()?, std::f32::consts::PI);

    // Out-of-bounds reads return an error instead of panicking.
    assert!(matches!(r.read_u8(), Err(WireError::BufferUnderflow { .. })));
    Ok(())
}
```

## Status

Pre-release (`0.5.0-beta`). The API is functional and unit-tested but may still change before a stable release.

Licensed under MIT.
