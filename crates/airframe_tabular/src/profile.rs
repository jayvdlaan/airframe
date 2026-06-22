use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Description of one tabular source.
///
/// Loadable from TOML — column maps are nested tables for readability:
///
/// ```toml
/// delimiter = ";"
/// has_header = true
/// skip_lines = 0
///
/// [columns]
/// date   = "Booking Date"
/// amount = "Amount (EUR)"
/// payee  = "Counterparty"
/// memo   = "Description"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Field delimiter byte. TOML carries a single-char string; converted in `Profile::resolve`.
    #[serde(default = "default_delimiter", with = "byte_char")]
    pub delimiter: u8,

    /// Quote byte. Same string-to-byte convention as `delimiter`.
    #[serde(default = "default_quote", with = "byte_char")]
    pub quote: u8,

    #[serde(default = "default_true")]
    pub has_header: bool,

    /// Skip N raw lines before parsing (banks often include metadata headers).
    #[serde(default)]
    pub skip_lines: usize,

    /// Source encoding label per the WHATWG Encoding standard
    /// (`"utf-8"`, `"windows-1252"`, `"iso-8859-1"`, …). Default `"utf-8"`.
    #[serde(default = "default_encoding")]
    pub encoding: String,

    #[serde(default)]
    pub columns: ColumnMap,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ColumnMap {
    /// `logical_name` → `csv_header_name` (or 0-based index string when `has_header = false`).
    #[serde(flatten)]
    pub fields: BTreeMap<String, String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            delimiter: default_delimiter(),
            quote: default_quote(),
            has_header: true,
            skip_lines: 0,
            encoding: default_encoding(),
            columns: ColumnMap::default(),
        }
    }
}

fn default_delimiter() -> u8 {
    b','
}
fn default_quote() -> u8 {
    b'"'
}
fn default_true() -> bool {
    true
}
fn default_encoding() -> String {
    "utf-8".to_string()
}

/// Serde helpers to express a single byte as a one-character string in TOML.
mod byte_char {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &u8, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&(*v as char).to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
        let s = String::deserialize(d)?;
        let mut iter = s.chars();
        let c = iter.next().ok_or_else(|| D::Error::custom("empty"))?;
        if iter.next().is_some() || !c.is_ascii() {
            return Err(D::Error::custom("expected single ASCII char"));
        }
        Ok(c as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_profile_from_toml() {
        let src = r#"
            delimiter = ";"
            has_header = true
            skip_lines = 2

            [columns]
            date   = "Booking Date"
            amount = "Amount (EUR)"
            payee  = "Counterparty"
        "#;
        let p: Profile = toml::from_str(src).unwrap();
        assert_eq!(p.delimiter, b';');
        assert_eq!(p.skip_lines, 2);
        assert_eq!(p.columns.fields.get("date").unwrap(), "Booking Date");
        assert_eq!(p.columns.fields.get("amount").unwrap(), "Amount (EUR)");
    }
}
