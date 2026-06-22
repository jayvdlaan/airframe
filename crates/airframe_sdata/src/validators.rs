use regex::Regex;
use serde_json::Value;

use crate::error::{AirframeSdataError, Result};

pub struct Validators;

impl Validators {
    pub fn require_fields(value: &Value, fields: &[&str]) -> Result<()> {
        let obj = value
            .as_object()
            .ok_or_else(|| AirframeSdataError::ValidationError("expected object".into()))?;
        for f in fields {
            if !obj.contains_key(*f) {
                return Err(AirframeSdataError::ValidationError(format!(
                    "missing required field '{}'",
                    f
                )));
            }
        }
        Ok(())
    }

    pub fn enum_field(value: &Value, field: &str, allowed: &[&str]) -> Result<()> {
        let s = value.get(field).and_then(|v| v.as_str()).ok_or_else(|| {
            AirframeSdataError::ValidationError(format!("field '{}' must be string", field))
        })?;
        if !allowed.iter().any(|a| a == &s) {
            return Err(AirframeSdataError::ValidationError(format!(
                "field '{}' invalid value '{}'",
                field, s
            )));
        }
        Ok(())
    }

    pub fn range_field_i64(value: &Value, field: &str, min: i64, max: i64) -> Result<()> {
        let n = value.get(field).and_then(|v| v.as_i64()).ok_or_else(|| {
            AirframeSdataError::ValidationError(format!("field '{}' must be integer", field))
        })?;
        if n < min || n > max {
            return Err(AirframeSdataError::ValidationError(format!(
                "field '{}' out of range [{}..={}]",
                field, min, max
            )));
        }
        Ok(())
    }

    pub fn regex_field(value: &Value, field: &str, regex: &Regex) -> Result<()> {
        let s = value.get(field).and_then(|v| v.as_str()).ok_or_else(|| {
            AirframeSdataError::ValidationError(format!("field '{}' must be string", field))
        })?;
        if !regex.is_match(s) {
            return Err(AirframeSdataError::ValidationError(format!(
                "field '{}' does not match regex",
                field
            )));
        }
        Ok(())
    }
}

// Minimal JSON DSL-like runner: [{"type":"required","fields":["a","b"]}, ...]
pub fn run_minidsl(value: &Value, rules: &Value) -> Result<()> {
    let arr = rules
        .as_array()
        .ok_or_else(|| AirframeSdataError::ValidationError("rules must array".into()))?;
    for rule in arr {
        let typ = rule
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AirframeSdataError::ValidationError("rule requires 'type'".into()))?;
        match typ {
            "required" => {
                let fields = rule
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        AirframeSdataError::ValidationError("required.fields must array".into())
                    })?;
                let names: Vec<&str> = fields.iter().filter_map(|v| v.as_str()).collect();
                Validators::require_fields(value, &names)?;
            }
            "enum" => {
                let field = rule
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AirframeSdataError::ValidationError("enum.field".into()))?;
                let values = rule
                    .get("values")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| AirframeSdataError::ValidationError("enum.values".into()))?;
                let names: Vec<&str> = values.iter().filter_map(|v| v.as_str()).collect();
                Validators::enum_field(value, field, &names)?;
            }
            "range" => {
                let field = rule
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AirframeSdataError::ValidationError("range.field".into()))?;
                let min = rule
                    .get("min")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| AirframeSdataError::ValidationError("range.min".into()))?;
                let max = rule
                    .get("max")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| AirframeSdataError::ValidationError("range.max".into()))?;
                Validators::range_field_i64(value, field, min, max)?;
            }
            "regex" => {
                let field = rule
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AirframeSdataError::ValidationError("regex.field".into()))?;
                let pat = rule
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AirframeSdataError::ValidationError("regex.pattern".into()))?;
                let re = Regex::new(pat)
                    .map_err(|e| AirframeSdataError::ValidationError(e.to_string()))?;
                Validators::regex_field(value, field, &re)?;
            }
            other => {
                return Err(AirframeSdataError::ValidationError(format!(
                    "unknown rule type '{}'",
                    other
                )))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn require_fields_ok_and_missing() {
        let v = json!({"a":1, "b":2});
        assert!(Validators::require_fields(&v, &["a", "b"]).is_ok());
        let err = Validators::require_fields(&v, &["a", "c"]).unwrap_err();
        match err {
            AirframeSdataError::ValidationError(s) => assert!(s.contains("missing required field")),
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn enum_field_ok_and_bad() {
        let v = json!({"state":"on"});
        Validators::enum_field(&v, "state", &["on", "off"]).unwrap();
        let err = Validators::enum_field(&v, "state", &["off"]).unwrap_err();
        match err {
            AirframeSdataError::ValidationError(s) => assert!(s.contains("invalid value")),
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn range_and_regex() {
        let v = json!({"n": 5, "name":"user_01"});
        Validators::range_field_i64(&v, "n", 1, 10).unwrap();
        let err = Validators::range_field_i64(&v, "n", 6, 10).unwrap_err();
        match err {
            AirframeSdataError::ValidationError(s) => assert!(s.contains("out of range")),
            _ => panic!("expected validation error"),
        }

        let re = Regex::new(r"^user_\d+").unwrap();
        Validators::regex_field(&v, "name", &re).unwrap();
        let bad = Regex::new(r"^admin").unwrap();
        let err = Validators::regex_field(&v, "name", &bad).unwrap_err();
        match err {
            AirframeSdataError::ValidationError(s) => assert!(s.contains("does not match regex")),
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn mini_dsl_runner() {
        let v = json!({"kind":"alpha","n":7,"code":"A9"});
        let rules = json!([
            {"type":"required","fields":["kind","n"]},
            {"type":"enum","field":"kind","values":["alpha","beta"]},
            {"type":"range","field":"n","min":1,"max":10},
            {"type":"regex","field":"code","pattern":"^[A-Z]\\d$"}
        ]);
        run_minidsl(&v, &rules).unwrap();

        // Unknown rule type triggers validation error
        let bad_rules = json!([{"type":"unknown"}]);
        assert!(matches!(
            run_minidsl(&v, &bad_rules),
            Err(AirframeSdataError::ValidationError(_))
        ));
    }
}
