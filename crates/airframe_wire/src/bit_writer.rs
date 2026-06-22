/// Bitwise writer that packs values into a byte buffer at the bit level.
#[derive(Debug, Clone)]
pub struct BitWriter {
    buffer: Vec<u8>,
    bit_pos: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            bit_pos: 0,
        }
    }

    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(bytes),
            bit_pos: 0,
        }
    }

    /// Current write position in bits.
    pub fn bit_position(&self) -> usize {
        self.bit_pos
    }

    /// Write a single bit.
    pub fn write_bit(&mut self, value: bool) {
        let byte_index = self.bit_pos / 8;
        let bit_offset = 7 - (self.bit_pos % 8); // MSB first

        if byte_index >= self.buffer.len() {
            self.buffer.push(0);
        }

        if value {
            self.buffer[byte_index] |= 1 << bit_offset;
        }

        self.bit_pos += 1;
    }

    /// Write `num_bits` bits from a u64 value, MSB first.
    pub fn write_bits(&mut self, value: u64, num_bits: u8) {
        debug_assert!(num_bits <= 64);
        for i in (0..num_bits).rev() {
            self.write_bit((value >> i) & 1 == 1);
        }
    }

    pub fn write_u8(&mut self, value: u8) {
        self.write_bits(value as u64, 8);
    }

    pub fn write_u16(&mut self, value: u16) {
        self.write_bits(value as u64, 16);
    }

    pub fn write_u32(&mut self, value: u32) {
        self.write_bits(value as u64, 32);
    }

    pub fn write_u64(&mut self, value: u64) {
        self.write_bits(value, 64);
    }

    pub fn write_i8(&mut self, value: i8) {
        self.write_u8(value as u8);
    }

    pub fn write_i16(&mut self, value: i16) {
        self.write_u16(value as u16);
    }

    pub fn write_i32(&mut self, value: i32) {
        self.write_u32(value as u32);
    }

    pub fn write_f32(&mut self, value: f32) {
        self.write_u32(f32::to_bits(value));
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_bit(value);
    }

    /// Write a length-prefixed byte slice (u16 length + raw bytes).
    pub fn write_bytes(&mut self, data: &[u8]) {
        self.write_u16(data.len() as u16);
        for &b in data {
            self.write_u8(b);
        }
    }

    /// Write a length-prefixed UTF-8 string (u16 length + bytes).
    pub fn write_string(&mut self, s: &str) {
        self.write_bytes(s.as_bytes());
    }

    /// Write a variable-length encoded u32 (7-bit groups, high bit = continuation).
    pub fn write_var_u32(&mut self, mut value: u32) {
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            self.write_u8(byte);
            if value == 0 {
                break;
            }
        }
    }

    /// Consume the writer and return the byte buffer.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}
