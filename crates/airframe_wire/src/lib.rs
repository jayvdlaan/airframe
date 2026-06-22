//! Bit-level binary protocol primitives.
//!
//! Provides [`BitWriter`] and [`BitReader`] for packing and unpacking values
//! at the bit level, with support for sub-byte fields, variable-length integers,
//! and length-prefixed strings/byte arrays.

pub mod bit_reader;
pub mod bit_writer;
pub mod error;

pub use bit_reader::BitReader;
pub use bit_writer::BitWriter;
pub use error::WireError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_single_bits() {
        let mut w = BitWriter::new();
        w.write_bit(true);
        w.write_bit(false);
        w.write_bit(true);
        w.write_bit(true);
        w.write_bit(false);
        w.write_bit(true);
        w.write_bit(false);
        w.write_bit(false);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);

        assert!(r.read_bit().unwrap());
        assert!(!r.read_bit().unwrap());
        assert!(r.read_bit().unwrap());
        assert!(r.read_bit().unwrap());
        assert!(!r.read_bit().unwrap());
        assert!(r.read_bit().unwrap());
        assert!(!r.read_bit().unwrap());
        assert!(!r.read_bit().unwrap());
    }

    #[test]
    fn roundtrip_u8() {
        let mut w = BitWriter::new();
        w.write_u8(0);
        w.write_u8(127);
        w.write_u8(255);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_u8().unwrap(), 0);
        assert_eq!(r.read_u8().unwrap(), 127);
        assert_eq!(r.read_u8().unwrap(), 255);
    }

    #[test]
    fn roundtrip_u16() {
        let mut w = BitWriter::new();
        w.write_u16(0);
        w.write_u16(1234);
        w.write_u16(u16::MAX);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_u16().unwrap(), 0);
        assert_eq!(r.read_u16().unwrap(), 1234);
        assert_eq!(r.read_u16().unwrap(), u16::MAX);
    }

    #[test]
    fn roundtrip_u32() {
        let mut w = BitWriter::new();
        w.write_u32(0);
        w.write_u32(0xDEAD_BEEF);
        w.write_u32(u32::MAX);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_u32().unwrap(), 0);
        assert_eq!(r.read_u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(r.read_u32().unwrap(), u32::MAX);
    }

    #[test]
    fn roundtrip_u64() {
        let mut w = BitWriter::new();
        w.write_u64(0);
        w.write_u64(0xDEAD_BEEF_CAFE_BABE);
        w.write_u64(u64::MAX);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_u64().unwrap(), 0);
        assert_eq!(r.read_u64().unwrap(), 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(r.read_u64().unwrap(), u64::MAX);
    }

    #[test]
    fn roundtrip_signed_integers() {
        let mut w = BitWriter::new();
        w.write_i8(-1);
        w.write_i8(i8::MIN);
        w.write_i8(i8::MAX);
        w.write_i16(-1234);
        w.write_i16(i16::MIN);
        w.write_i32(-100_000);
        w.write_i32(i32::MIN);
        w.write_i32(i32::MAX);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_i8().unwrap(), -1);
        assert_eq!(r.read_i8().unwrap(), i8::MIN);
        assert_eq!(r.read_i8().unwrap(), i8::MAX);
        assert_eq!(r.read_i16().unwrap(), -1234);
        assert_eq!(r.read_i16().unwrap(), i16::MIN);
        assert_eq!(r.read_i32().unwrap(), -100_000);
        assert_eq!(r.read_i32().unwrap(), i32::MIN);
        assert_eq!(r.read_i32().unwrap(), i32::MAX);
    }

    #[test]
    fn roundtrip_f32() {
        let mut w = BitWriter::new();
        w.write_f32(0.0);
        w.write_f32(std::f32::consts::PI);
        w.write_f32(-1.5);
        w.write_f32(f32::INFINITY);
        w.write_f32(f32::NEG_INFINITY);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_f32().unwrap(), 0.0);
        assert_eq!(r.read_f32().unwrap(), std::f32::consts::PI);
        assert_eq!(r.read_f32().unwrap(), -1.5);
        assert_eq!(r.read_f32().unwrap(), f32::INFINITY);
        assert_eq!(r.read_f32().unwrap(), f32::NEG_INFINITY);
    }

    #[test]
    fn roundtrip_bool() {
        let mut w = BitWriter::new();
        w.write_bool(true);
        w.write_bool(false);
        w.write_bool(true);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert!(r.read_bool().unwrap());
        assert!(!r.read_bool().unwrap());
        assert!(r.read_bool().unwrap());
    }

    #[test]
    fn roundtrip_bytes() {
        let mut w = BitWriter::new();
        w.write_bytes(&[]);
        w.write_bytes(&[1, 2, 3, 4, 5]);
        w.write_bytes(&[0xFF; 256]);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bytes().unwrap(), &[]);
        assert_eq!(r.read_bytes().unwrap(), &[1, 2, 3, 4, 5]);
        assert_eq!(r.read_bytes().unwrap(), &[0xFF; 256]);
    }

    #[test]
    fn roundtrip_strings() {
        let mut w = BitWriter::new();
        w.write_string("");
        w.write_string("hello");
        w.write_string("unicode: \u{1F600}\u{1F4A9}");

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_string().unwrap(), "");
        assert_eq!(r.read_string().unwrap(), "hello");
        assert_eq!(r.read_string().unwrap(), "unicode: \u{1F600}\u{1F4A9}");
    }

    #[test]
    fn roundtrip_var_u32() {
        let test_values: &[u32] = &[0, 1, 127, 128, 16383, 16384, 2_097_151, u32::MAX];
        let mut w = BitWriter::new();
        for &v in test_values {
            w.write_var_u32(v);
        }

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        for &expected in test_values {
            assert_eq!(r.read_var_u32().unwrap(), expected);
        }
    }

    #[test]
    fn mixed_writes_and_reads() {
        let mut w = BitWriter::new();
        w.write_u8(42);
        w.write_bool(true);
        w.write_u16(1000);
        w.write_string("test");
        w.write_f32(core::f32::consts::PI);
        w.write_var_u32(999);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_u8().unwrap(), 42);
        assert!(r.read_bool().unwrap());
        assert_eq!(r.read_u16().unwrap(), 1000);
        assert_eq!(r.read_string().unwrap(), "test");
        assert_eq!(r.read_f32().unwrap(), core::f32::consts::PI);
        assert_eq!(r.read_var_u32().unwrap(), 999);
    }

    #[test]
    fn sub_byte_bit_writes() {
        let mut w = BitWriter::new();
        w.write_bits(0b101, 3);
        w.write_bits(0b10011, 5);

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(5).unwrap(), 0b10011);
    }

    #[test]
    fn sub_byte_mixed_sizes() {
        let mut w = BitWriter::new();
        w.write_bits(0b1, 1);
        w.write_bits(0b0110, 4);
        w.write_bits(0b111, 3);

        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 1);

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bits(1).unwrap(), 1);
        assert_eq!(r.read_bits(4).unwrap(), 0b0110);
        assert_eq!(r.read_bits(3).unwrap(), 0b111);
    }

    #[test]
    fn buffer_underflow_error() {
        let data = [0u8; 1];
        let mut r = BitReader::new(&data);
        assert!(r.read_u8().is_ok());
        let err = r.read_bit().unwrap_err();
        assert!(matches!(err, WireError::BufferUnderflow { .. }));
    }

    #[test]
    fn buffer_underflow_partial() {
        let data = [0u8; 1];
        let mut r = BitReader::new(&data);
        assert!(r.read_bits(3).is_ok());
        let err = r.read_u8().unwrap_err();
        assert!(matches!(err, WireError::BufferUnderflow { .. }));
    }

    #[test]
    fn remaining_bits_tracking() {
        let data = [0u8; 2];
        let mut r = BitReader::new(&data);
        assert_eq!(r.remaining_bits(), 16);
        r.read_bit().unwrap();
        assert_eq!(r.remaining_bits(), 15);
        r.read_u8().unwrap();
        assert_eq!(r.remaining_bits(), 7);
    }

    #[test]
    fn bit_position_tracking() {
        let mut w = BitWriter::new();
        assert_eq!(w.bit_position(), 0);
        w.write_bit(true);
        assert_eq!(w.bit_position(), 1);
        w.write_u8(0);
        assert_eq!(w.bit_position(), 9);
        w.write_u16(0);
        assert_eq!(w.bit_position(), 25);

        let data = [0u8; 4];
        let mut r = BitReader::new(&data);
        assert_eq!(r.bit_position(), 0);
        r.read_bit().unwrap();
        assert_eq!(r.bit_position(), 1);
        r.read_u8().unwrap();
        assert_eq!(r.bit_position(), 9);
    }

    #[test]
    fn invalid_utf8_string() {
        let mut w = BitWriter::new();
        let invalid_bytes: &[u8] = &[0xFF, 0xFE, 0x80];
        w.write_u16(invalid_bytes.len() as u16);
        for &b in invalid_bytes {
            w.write_u8(b);
        }

        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        let err = r.read_string().unwrap_err();
        assert!(matches!(err, WireError::DecodeError(_)));
    }
}
