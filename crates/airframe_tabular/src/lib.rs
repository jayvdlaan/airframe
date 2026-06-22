//! airframe_tabular — config-driven tabular ingest (CSV/TSV) → typed rows.
//!
//! See README.md for the design rationale. The crate is a thin L1 primitive:
//! a [`Profile`] describes how to read one source, [`read_rows`] runs it, and
//! consumers do their own domain mapping on top.

pub mod encoding;
pub mod error;
pub mod parse;
pub mod profile;
pub mod row;

pub use encoding::decode;
pub use error::TabularError;
pub use profile::{ColumnMap, Profile};
pub use row::Row;

use std::io::Cursor;

/// Read all rows from `bytes` according to `profile`.
///
/// Returns one [`Row`] per data record (header excluded if
/// `profile.has_header` is set). Unknown logical fields in `profile.columns`
/// that do not resolve to a header position cause [`TabularError::MissingColumn`].
pub fn read_rows(bytes: &[u8], profile: &Profile) -> Result<Vec<Row>, TabularError> {
    let decoded = decode(bytes, &profile.encoding)?;
    let utf8_bytes = decoded.as_bytes();
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(profile.delimiter)
        .quote(profile.quote)
        .has_headers(profile.has_header)
        .flexible(true)
        .from_reader(skipped(utf8_bytes, profile.skip_lines));

    let header_index = if profile.has_header {
        let headers = reader.headers().map_err(TabularError::from)?.clone();
        let mut idx = std::collections::HashMap::new();
        for (i, h) in headers.iter().enumerate() {
            idx.insert(h.trim().to_string(), i);
        }
        idx
    } else {
        // Without headers, the column map values are interpreted as 0-based positions
        std::collections::HashMap::new()
    };

    let mut field_positions: Vec<(String, usize)> =
        Vec::with_capacity(profile.columns.fields.len());
    for (logical, header) in &profile.columns.fields {
        let pos = if profile.has_header {
            header_index
                .get(header.trim())
                .copied()
                .ok_or_else(|| TabularError::MissingColumn(header.clone()))?
        } else {
            header
                .parse::<usize>()
                .map_err(|_| TabularError::MissingColumn(header.clone()))?
        };
        field_positions.push((logical.clone(), pos));
    }

    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(TabularError::from)?;
        let mut row = Row::empty(i);
        for (logical, pos) in &field_positions {
            let value = record.get(*pos).unwrap_or("").trim().to_string();
            row.set(logical.clone(), value);
        }
        rows.push(row);
    }
    Ok(rows)
}

fn skipped(bytes: &[u8], skip_lines: usize) -> Cursor<Vec<u8>> {
    if skip_lines == 0 {
        return Cursor::new(bytes.to_vec());
    }
    let mut remaining = skip_lines;
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if *b == b'\n' {
            remaining -= 1;
            start = i + 1;
        }
    }
    Cursor::new(bytes[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_for(fields: &[(&str, &str)]) -> Profile {
        let mut p = Profile::default();
        for (logical, header) in fields {
            p.columns
                .fields
                .insert((*logical).to_string(), (*header).to_string());
        }
        p
    }

    #[test]
    fn reads_simple_csv() {
        let csv =
            b"date,amount,payee\n2026-01-02,12.50,Albert Heijn\n2026-01-03,-5.00,Coffee Shop\n";
        let profile = profile_for(&[("date", "date"), ("amount", "amount"), ("payee", "payee")]);
        let rows = read_rows(csv, &profile).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("date"), Some("2026-01-02"));
        assert_eq!(rows[0].get("payee"), Some("Albert Heijn"));
        assert_eq!(rows[1].get("amount"), Some("-5.00"));
    }

    #[test]
    fn skips_metadata_lines() {
        let csv = b"# Statement export\n# Opening: 100.00\ndate,amount\n2026-01-02,12.50\n";
        let mut profile = profile_for(&[("date", "date"), ("amount", "amount")]);
        profile.skip_lines = 2;
        let rows = read_rows(csv, &profile).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("date"), Some("2026-01-02"));
    }

    #[test]
    fn missing_column_errors() {
        let csv = b"date,amount\n2026-01-02,12.50\n";
        let profile = profile_for(&[("date", "date"), ("amount", "amount"), ("payee", "payee")]);
        let err = read_rows(csv, &profile).unwrap_err();
        assert!(matches!(err, TabularError::MissingColumn(ref s) if s == "payee"));
    }

    #[test]
    fn semicolon_delimiter() {
        let csv = b"date;amount\n2026-01-02;12,50\n";
        let mut profile = profile_for(&[("date", "date"), ("amount", "amount")]);
        profile.delimiter = b';';
        let rows = read_rows(csv, &profile).unwrap();
        assert_eq!(rows[0].get("amount"), Some("12,50"));
    }
}
