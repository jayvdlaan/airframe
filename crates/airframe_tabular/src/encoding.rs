//! Source-encoding handling. Wraps `encoding_rs` so callers don't need it
//! directly.

use crate::error::TabularError;
use std::borrow::Cow;

/// Decode `bytes` from `label` into a `Cow<str>`.
///
/// `label` follows the WHATWG Encoding standard (`"utf-8"`, `"windows-1252"`,
/// `"iso-8859-1"`, …). Unknown labels return [`TabularError::UnknownEncoding`].
/// Malformed sequences are replaced with U+FFFD.
pub fn decode<'a>(bytes: &'a [u8], label: &str) -> Result<Cow<'a, str>, TabularError> {
    let enc = encoding_rs::Encoding::for_label(label.as_bytes())
        .ok_or_else(|| TabularError::UnknownEncoding(label.to_string()))?;
    let (cow, _used_enc, _had_errors) = enc.decode(bytes);
    Ok(cow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_passthrough() {
        let s = decode("Albert Heijn".as_bytes(), "utf-8").unwrap();
        assert_eq!(s, "Albert Heijn");
    }

    #[test]
    fn decodes_windows_1252() {
        // 0xEB is ë in windows-1252 (and latin1).
        let bytes = b"initi\xEBrende";
        let s = decode(bytes, "windows-1252").unwrap();
        assert_eq!(s, "initiërende");
    }

    #[test]
    fn unknown_label_errors() {
        let err = decode(b"x", "definitely-not-an-encoding").unwrap_err();
        assert!(matches!(err, TabularError::UnknownEncoding(_)));
    }
}
