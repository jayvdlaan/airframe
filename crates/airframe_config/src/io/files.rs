//! File-based configuration sources.
//!
//! Implements file reading/merging and basic TOML helpers.
// When `module` feature is disabled, these functions may be unused.
#![cfg_attr(not(feature = "module"), allow(dead_code))]
use std::path::PathBuf;
use tracing::warn;

pub(crate) fn merge_toml(dst: &mut toml::Value, src: toml::Value) {
    // Wrapper to keep public signature; delegates to recursive implementation that tracks key path for diagnostics
    fn do_merge(dst: &mut toml::Value, src: toml::Value, path: &mut Vec<String>) {
        use std::mem::discriminant;
        use toml::Value::*;
        match (dst, src) {
            (Table(dst_tbl), Table(src_tbl)) => {
                for (k, v) in src_tbl.into_iter() {
                    match dst_tbl.get_mut(&k) {
                        Some(existing) => {
                            path.push(k.clone());
                            do_merge(existing, v, path);
                            path.pop();
                        }
                        None => {
                            // Unknown key being introduced during merge
                            let key = if path.is_empty() {
                                k.clone()
                            } else {
                                format!("{}.{}", path.join("."), k)
                            };
                            warn!(target = "airframe_config", key = %key, "unknown config key");
                            dst_tbl.insert(k, v);
                        }
                    }
                }
            }
            (Array(dst_arr), Array(mut src_arr)) => {
                // overwrite array (last wins) per layering semantics
                *dst_arr = std::mem::take(&mut src_arr);
            }
            (dst_slot, other) => {
                // Overwriting a non-table/non-array value. Warn if type changes.
                let key = path.join(".");
                if discriminant(dst_slot) != discriminant(&other) {
                    warn!(target = "airframe_config", key = %key, "unknown config key");
                }
                *dst_slot = other;
            }
        }
    }
    do_merge(dst, src, &mut Vec::new());
}

pub(crate) fn set_path(dst: &mut toml::Value, path: &[&str], value: toml::Value) {
    if path.is_empty() {
        *dst = value;
        return;
    }
    let mut node = dst;
    for (i, key) in path.iter().enumerate() {
        let is_last = i == path.len() - 1;
        if is_last {
            // ensure table
            if !node.is_table() {
                // type mismatch when trying to set a nested key on a non-table
                let key_path = path[..i].join(".");
                warn!(target = "airframe_config", key = %key_path, "unknown config key");
                *node = toml::Value::Table(Default::default());
            }
            if let Some(tbl) = node.as_table_mut() {
                if !tbl.contains_key(*key) {
                    let key_path = if i == 0 {
                        (*key).to_string()
                    } else {
                        format!("{}.{}", path[..i].join("."), key)
                    };
                    warn!(target = "airframe_config", key = %key_path, "unknown config key");
                }
                tbl.insert((*key).to_string(), value.clone());
            }
        } else {
            if !node.is_table() {
                let key_path = path[..i].join(".");
                warn!(target = "airframe_config", key = %key_path, "unknown config key");
                *node = toml::Value::Table(Default::default());
            }
            let tbl = node.as_table_mut().unwrap();
            node = tbl
                .entry((*key).to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
        }
    }
}

pub(crate) fn parse_scalar(s: &str) -> toml::Value {
    let trimmed = s.trim();
    // Very lightweight array parsing for CLI/env overrides: [a,b,c]
    // Items are split by comma and individually parsed via parse_scalar again (bool/int/float/string).
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        // Support empty list: []
        if inner.trim().is_empty() {
            return toml::Value::Array(vec![]);
        }
        let items: Vec<toml::Value> = inner.split(',').map(|t| parse_scalar(t.trim())).collect();
        return toml::Value::Array(items);
    }

    if let Ok(b) = trimmed.parse::<bool>() {
        return toml::Value::Boolean(b);
    }
    if let Ok(i) = trimmed.parse::<i64>() {
        return toml::Value::Integer(i);
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return toml::Value::Float(f);
    }
    toml::Value::String(trimmed.to_string())
}

pub(crate) fn read_files(paths: &[PathBuf]) -> toml::Value {
    let mut acc = toml::Value::Table(Default::default());
    for p in paths {
        match std::fs::read_to_string(p) {
            Ok(s) => {
                // Try TOML first
                if let Ok(v) = s.parse::<toml::Value>() {
                    merge_toml(&mut acc, v);
                    continue;
                }
                // Try JSON next
                if let Ok(json_v) = serde_json::from_str::<serde_json::Value>(&s) {
                    let tv = json_to_toml(&json_v);
                    merge_toml(&mut acc, tv);
                    continue;
                }
                // Try YAML last
                if let Ok(yaml_v) = serde_yaml::from_str::<serde_yaml::Value>(&s) {
                    if let Ok(json_v) = serde_json::to_value(yaml_v) {
                        let tv = json_to_toml(&json_v);
                        merge_toml(&mut acc, tv);
                        continue;
                    }
                }
                // A config file that was explicitly listed but cannot be parsed must
                // never be silently dropped: that is a fail-open where security
                // settings quietly revert to defaults. Surface it loudly.
                tracing::error!(
                    target = "airframe_config",
                    path = %p.display(),
                    "config file could not be parsed as TOML/JSON/YAML; it was IGNORED and settings may fall back to defaults"
                );
            }
            Err(e) => {
                tracing::error!(
                    target = "airframe_config",
                    path = %p.display(),
                    error = %e,
                    "failed to read config file; it was IGNORED and settings may fall back to defaults"
                );
            }
        }
    }
    acc
}

fn json_to_toml(v: &serde_json::Value) -> toml::Value {
    match v {
        serde_json::Value::Null => toml::Value::String(String::new()),
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                toml::Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Array(arr) => toml::Value::Array(arr.iter().map(json_to_toml).collect()),
        serde_json::Value::Object(map) => {
            let mut tbl = toml::map::Map::new();
            for (k, vv) in map.iter() {
                tbl.insert(k.clone(), json_to_toml(vv));
            }
            toml::Value::Table(tbl)
        }
    }
}

pub(crate) fn split_paths(s: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    let sep = ';';
    #[cfg(not(windows))]
    let sep = ':';
    s.split(sep)
        .filter(|p| !p.trim().is_empty())
        .map(|p| PathBuf::from(p.trim()))
        .collect()
}
