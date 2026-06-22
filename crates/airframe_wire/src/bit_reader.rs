use crate::error::WireError;

/// Bitwise reader that unpacks values from a byte buffer at the bit level.
#[derive(Debug, Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    /// Current read position in bits.
    pub fn bit_position(&self) -> usize {
        self.bit_pos
    }

    /// Remaining bits available to read.
    pub fn remaining_bits(&self) -> usize {
        self.data.len() * 8 - self.bit_pos
    }

    fn check_available(&self, needed: usize) -> Result<(), WireError> {
        let available = self.remaining_bits();
        if available < needed {
            Err(WireError::BufferUnderflow { needed, available })
        } else {
            Ok(())
        }
    }

    /// Read a single bit.
    pub fn read_bit(&mut self) -> Result<bool, WireError> {
        self.check_available(1)?;
        let byte_index = self.bit_pos / 8;
        let bit_offset = 7 - (self.bit_pos % 8); // MSB first
        let value = (self.data[byte_index] >> bit_offset) & 1 == 1;
        self.bit_pos += 1;
        Ok(value)
    }

    /// Read `num_bits` bits into a u64, MSB first.
    pub fn read_bits(&mut self, num_bits: u8) -> Result<u64, WireError> {
        debug_assert!(num_bits <= 64);
        self.check_available(num_bits as usize)?;
        let mut value: u64 = 0;
        for _ in 0..num_bits {
            value <<= 1;
            if self.read_bit()? {
                value |= 1;
            }
        }
        Ok(value)
    }

    pub fn read_u8(&mut self) -> Result<u8, WireError> {
        Ok(self.read_bits(8)? as u8)
    }

    pub fn read_u16(&mut self) -> Result<u16, WireError> {
        Ok(self.read_bits(16)? as u16)
    }

    pub fn read_u32(&mut self) -> Result<u32, WireError> {
        Ok(self.read_bits(32)? as u32)
    }

    pub fn read_u64(&mut self) -> Result<u64, WireError> {
        self.read_bits(64)
    }

    pub fn read_i8(&mut self) -> Result<i8, WireError> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_i16(&mut self) -> Result<i16, WireError> {
        Ok(self.read_u16()? as i16)
    }

    pub fn read_i32(&mut self) -> Result<i32, WireError> {
        Ok(self.read_u32()? as i32)
    }

    pub fn read_f32(&mut self) -> Result<f32, WireError> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    pub fn read_bool(&mut self) -> Result<bool, WireError> {
        self.read_bit()
    }

    /// Read a length-prefixed byte slice (u16 length + raw bytes).
    pub fn read_bytes(&mut self) -> Result<Vec<u8>, WireError> {
        let len = self.read_u16()? as usize;
        self.check_available(len * 8)?;
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            buf.push(self.read_u8()?);
        }
        Ok(buf)
    }

    /// Read a length-prefixed UTF-8 string (u16 length + bytes).
    pub fn read_string(&mut self) -> Result<String, WireError> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes).map_err(|e| WireError::DecodeError(format!("invalid UTF-8: {e}")))
    }

    /// Read a variable-length encoded u32 (7-bit groups, high bit = continuation).
    pub fn read_var_u32(&mut self) -> Result<u32, WireError> {
        let mut value: u32 = 0;
        let mut shift: u32 = 0;
        loop {
            let byte = self.read_u8()?;
            value |= ((byte & 0x7F) as u32) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 35 {
                return Err(WireError::DecodeError(
                    "var_u32 overflow: too many continuation bytes".into(),
                ));
            }
        }
        Ok(value)
    }
}
