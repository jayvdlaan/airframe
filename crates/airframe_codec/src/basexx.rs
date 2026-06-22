use base64::{engine::general_purpose, Engine as _};

pub fn base64_encode(data: &[u8]) -> String {
    general_purpose::STANDARD.encode(data)
}

pub fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    general_purpose::STANDARD.decode(s)
}

pub fn base32_encode(data: &[u8]) -> String {
    data_encoding::BASE32_NOPAD.encode(data)
}

pub fn base32_decode(s: &str) -> Result<Vec<u8>, data_encoding::DecodeError> {
    data_encoding::BASE32_NOPAD.decode(s.as_bytes())
}

pub fn base16_encode(data: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(data)
}

pub fn base16_decode(s: &str) -> Result<Vec<u8>, data_encoding::DecodeError> {
    data_encoding::HEXLOWER.decode(s.as_bytes())
}

pub enum Multibase {
    Base16,
    Base32,
    Base64,
}

pub fn multibase_encode(base: Multibase, data: &[u8]) -> String {
    match base {
        Multibase::Base16 => format!("f{}", base16_encode(data)), // 'f' per multibase for hex
        Multibase::Base32 => format!("b{}", base32_encode(data)), // 'b' for base32
        Multibase::Base64 => format!("m{}", base64_encode(data)), // 'm' for base64
    }
}
