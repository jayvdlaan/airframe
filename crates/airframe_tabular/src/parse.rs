//! Locale-aware date and decimal helpers.
//!
//! Consumers can ignore these and parse `Row::get(..)` strings themselves;
//! they exist because every CSV-ingest tool reinvents them.

use crate::error::TabularError;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::str::FromStr;

/// Parse `s` as a date using a `chrono`-style format string (`%Y-%m-%d`, `%d-%m-%Y`, …).
pub fn date(s: &str, format: &str) -> Result<NaiveDate, TabularError> {
    NaiveDate::parse_from_str(s, format).map_err(|e| TabularError::Parse {
        kind: "date",
        value: s.to_string(),
        reason: e.to_string(),
    })
}

/// Parse a US-format decimal: thousands separator `,`, decimal point `.` — `"1,234.56"`.
pub fn decimal_us(s: &str) -> Result<Decimal, TabularError> {
    let cleaned: String = s
        .chars()
        .filter(|c| *c != ',' && !c.is_whitespace())
        .collect();
    Decimal::from_str(&cleaned).map_err(|e| TabularError::Parse {
        kind: "decimal_us",
        value: s.to_string(),
        reason: e.to_string(),
    })
}

/// Parse a European-format decimal: thousands separator `.`, decimal comma `,` — `"1.234,56"`.
///
/// Also accepts plain values without thousands separators (`"12,50"` or `"12.50"`).
pub fn decimal_european(s: &str) -> Result<Decimal, TabularError> {
    let trimmed = s.trim();
    let last_comma = trimmed.rfind(',');
    let last_dot = trimmed.rfind('.');

    let normalized = match (last_comma, last_dot) {
        (Some(c), Some(d)) if c > d => {
            // European: dots are thousands, comma is decimal
            let mut t = trimmed.replace('.', "");
            t = t.replacen(',', ".", 1);
            t
        }
        (Some(_), None) => trimmed.replacen(',', ".", 1),
        _ => trimmed.to_string(),
    };

    Decimal::from_str(&normalized).map_err(|e| TabularError::Parse {
        kind: "decimal_european",
        value: s.to_string(),
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_date() {
        let d = date("2026-05-25", "%Y-%m-%d").unwrap();
        assert_eq!(d.to_string(), "2026-05-25");
    }

    #[test]
    fn parses_european_date() {
        let d = date("25-05-2026", "%d-%m-%Y").unwrap();
        assert_eq!(d.to_string(), "2026-05-25");
    }

    #[test]
    fn parses_european_decimal() {
        assert_eq!(
            decimal_european("12,50").unwrap(),
            Decimal::from_str("12.50").unwrap()
        );
        assert_eq!(
            decimal_european("1.234,56").unwrap(),
            Decimal::from_str("1234.56").unwrap()
        );
        assert_eq!(
            decimal_european("-1.234,56").unwrap(),
            Decimal::from_str("-1234.56").unwrap()
        );
        assert_eq!(
            decimal_european("12.50").unwrap(),
            Decimal::from_str("12.50").unwrap()
        );
    }

    #[test]
    fn parses_us_decimal() {
        assert_eq!(
            decimal_us("12.50").unwrap(),
            Decimal::from_str("12.50").unwrap()
        );
        assert_eq!(
            decimal_us("1,234.56").unwrap(),
            Decimal::from_str("1234.56").unwrap()
        );
    }
}
