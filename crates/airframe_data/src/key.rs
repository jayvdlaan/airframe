use crate::error::{AirframeDataError, Result};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

// Disallow path separators and control characters in keys
const DISALLOWED: &AsciiSet = &CONTROLS.add(b'/').add(b'\\').add(b'\0');

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(String);

impl Key {
    pub fn new<S: AsRef<str>>(s: S) -> Result<Self> {
        let s = s.as_ref();
        if s.is_empty() {
            return Err(AirframeDataError::KeyInvalid("empty".into()));
        }
        if s.as_bytes()
            .iter()
            .any(|b| *b == b'/' || *b == b'\\' || *b == 0)
        {
            return Err(AirframeDataError::KeyInvalid(
                "contains path separator or NUL".into(),
            ));
        }
        // Reject the reserved path components "." and ".." — they contain no
        // disallowed bytes, so they survive filename encoding unchanged and would
        // let a key escape (or alias) the backend's root directory.
        if s == "." || s == ".." {
            return Err(AirframeDataError::KeyInvalid(
                "reserved path component".into(),
            ));
        }
        // Additional sanity: trim whitespace
        if s.trim().is_empty() {
            return Err(AirframeDataError::KeyInvalid("whitespace".into()));
        }
        Ok(Key(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn encode_filename(&self) -> String {
        utf8_percent_encode(&self.0, DISALLOWED).to_string()
    }

    pub fn decode_filename<S: AsRef<str>>(encoded: S) -> Result<Self> {
        let decoded = percent_decode_str(encoded.as_ref())
            .decode_utf8()
            .map_err(|_| AirframeDataError::KeyInvalid("utf8".into()))?;
        Key::new(decoded.as_ref())
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_valid_key() {
        let key = Key::new("valid_key").unwrap();
        assert_eq!(key.as_str(), "valid_key");
    }

    #[test]
    fn new_empty_fails() {
        let result = Key::new("");
        assert!(result.is_err());
    }

    #[test]
    fn new_whitespace_only_fails() {
        let result = Key::new("   ");
        assert!(result.is_err());
    }

    #[test]
    fn new_with_forward_slash_fails() {
        let result = Key::new("path/to/key");
        assert!(result.is_err());
    }

    #[test]
    fn new_with_backslash_fails() {
        let result = Key::new("path\\to\\key");
        assert!(result.is_err());
    }

    #[test]
    fn new_with_nul_fails() {
        let result = Key::new("key\0with\0nul");
        assert!(result.is_err());
    }

    #[test]
    fn encode_and_decode_filename_roundtrip() {
        let key = Key::new("my-key").unwrap();
        let encoded = key.encode_filename();
        let decoded = Key::decode_filename(&encoded).unwrap();
        assert_eq!(key, decoded);
    }

    #[test]
    fn encode_filename_preserves_simple_keys() {
        let key = Key::new("simple-key").unwrap();
        let encoded = key.encode_filename();
        // Simple alphanumeric keys should remain unchanged
        assert_eq!(encoded, "simple-key");
    }

    #[test]
    fn display_shows_key_value() {
        let key = Key::new("display_test").unwrap();
        assert_eq!(format!("{}", key), "display_test");
    }

    #[test]
    fn key_equality() {
        let key1 = Key::new("same").unwrap();
        let key2 = Key::new("same").unwrap();
        let key3 = Key::new("different").unwrap();

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn key_clone() {
        let key1 = Key::new("clone_me").unwrap();
        let key2 = key1.clone();
        assert_eq!(key1, key2);
    }
}
