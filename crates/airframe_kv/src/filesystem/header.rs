use anyhow::{bail, Result};

// ------- On-disk header with CRC (initial) -------
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Header {
    pub ver: u16,                 // 1
    pub etag: u64,                // next etag to persist
    pub updated_at_millis: i64,   // unix millis
    pub ttl_deadline_millis: i64, // -1 for none
    pub value_len: u64,
}

impl Header {
    pub const MAGIC: [u8; 4] = *b"AFKV";
    pub const VER: u16 = 1;

    pub fn now_millis() -> i64 {
        spacetime_std_runtime::now_millis() as i64
    }

    pub fn encode_with_value(&self, value: &[u8]) -> Vec<u8> {
        use crc32fast::Hasher;
        let mut buf = Vec::with_capacity(4 + 2 + 4 + 8 + 8 + 8 + value.len());
        // magic
        buf.extend_from_slice(&Self::MAGIC);
        // version
        buf.extend_from_slice(&self.ver.to_le_bytes());
        // placeholder CRC32
        let crc_pos = buf.len();
        buf.extend_from_slice(&0u32.to_le_bytes());
        // rest of header (without crc)
        buf.extend_from_slice(&self.etag.to_le_bytes());
        buf.extend_from_slice(&self.updated_at_millis.to_le_bytes());
        buf.extend_from_slice(&self.ttl_deadline_millis.to_le_bytes());
        buf.extend_from_slice(&self.value_len.to_le_bytes());
        // compute CRC over header (including magic+ver+0-crc+rest) + value
        let mut hasher = Hasher::new();
        hasher.update(&buf);
        hasher.update(value);
        let crc = hasher.finalize();
        // write CRC back
        buf[crc_pos..crc_pos + 4].copy_from_slice(&crc.to_le_bytes());
        // append value
        buf.extend_from_slice(value);
        buf
    }

    pub fn decode_and_validate(bytes: &[u8]) -> Result<(Header, Vec<u8>)> {
        use crc32fast::Hasher;
        let min = 4 + 2 + 4 + 8 + 8 + 8 + 8; // magic+ver+crc+etag+updated+ttl+len + at least 0 value
        if bytes.len() < min {
            bail!("file too small");
        }
        let magic = &bytes[0..4];
        if magic != Self::MAGIC {
            bail!("bad magic");
        }
        let ver = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        if ver != Self::VER {
            bail!("unsupported version");
        }
        let crc_read = u32::from_le_bytes(bytes[6..10].try_into().unwrap());
        let etag = u64::from_le_bytes(bytes[10..18].try_into().unwrap());
        let updated_at_millis = i64::from_le_bytes(bytes[18..26].try_into().unwrap());
        let ttl_deadline_millis = i64::from_le_bytes(bytes[26..34].try_into().unwrap());
        let value_len = u64::from_le_bytes(bytes[34..42].try_into().unwrap()) as usize;
        if bytes.len() < 42 + value_len {
            bail!("truncated file");
        }
        let value = bytes[42..42 + value_len].to_vec();
        // validate CRC over header(with zeroed CRC field) + value to match encoder
        let mut header_for_crc = bytes[0..42].to_vec();
        header_for_crc[6..10].copy_from_slice(&0u32.to_le_bytes());
        let mut hasher = Hasher::new();
        hasher.update(&header_for_crc);
        hasher.update(&value);
        let crc_calc = hasher.finalize();
        if crc_calc != crc_read {
            bail!("crc mismatch");
        }
        Ok((
            Header {
                ver,
                etag,
                updated_at_millis,
                ttl_deadline_millis,
                value_len: value_len as u64,
            },
            value,
        ))
    }
}
