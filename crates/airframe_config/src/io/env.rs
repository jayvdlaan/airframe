//! Environment variable configuration sources.
//!
//! Implements environment variable merging for env keys with configurable
//! prefixes (default: `AIRFRAME__`).
// May be unused when `module` feature is disabled.
#![cfg_attr(not(feature = "module"), allow(dead_code))]
use crate::io::files::{parse_scalar, set_path};

/// Merge environment variables into the destination TOML value using the provided
/// list of allowed prefixes. Each prefix should include the trailing `__`, e.g. `"AIRFRAME__"`.
///
/// Mapping rules:
/// - Only variables starting with one of the allowed prefixes are considered.
/// - The prefix is removed. Remaining key segments are split by `__` and lowercased.
/// - `__` maps to dot path segments (e.g., `FOO__BAR__BAZ` -> `foo.bar.baz`).
/// - Values are parsed via `parse_scalar` (bools/ints/arrays/strings).
pub(crate) fn merge_from_env_with_prefixes(dst: &mut toml::Value, prefixes: &[String]) {
    if prefixes.is_empty() {
        return;
    }
    for (k, v) in std::env::vars() {
        let mut matched: Option<&str> = None;
        for p in prefixes {
            if let Some(rest) = k.strip_prefix(p) {
                matched = Some(rest);
                break;
            }
        }
        if let Some(rest) = matched {
            // Map double underscores to dots and lowercase keys to match dot-path semantics
            let lowered: Vec<String> = rest
                .split("__")
                .filter(|p| !p.is_empty())
                .map(|p| p.to_ascii_lowercase())
                .collect();
            if !lowered.is_empty() {
                let parts: Vec<&str> = lowered.iter().map(|s| s.as_str()).collect();
                let val = parse_scalar(&v);
                set_path(dst, &parts, val);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::merge_from_env_with_prefixes;
    use serial_test::serial;

    fn clear_env(keys: &[&str]) {
        for k in keys {
            std::env::remove_var(k);
        }
    }

    #[test]
    #[serial]
    fn env_overrides_double_underscore_means_dot() {
        // Ensure clean slate
        clear_env(&["AIRFRAME__SERVER__SECURITY__REQUIRE_MAC"]);

        std::env::set_var("AIRFRAME__SERVER__SECURITY__REQUIRE_MAC", "false");
        let mut dst = toml::Value::Table(Default::default());
        merge_from_env_with_prefixes(&mut dst, &["AIRFRAME__".to_string()]);

        let require_mac = dst["server"].as_table().unwrap()["security"]
            .as_table()
            .unwrap()["require_mac"]
            .as_bool();
        assert_eq!(require_mac, Some(false));

        // Cleanup
        clear_env(&["AIRFRAME__SERVER__SECURITY__REQUIRE_MAC"]);
    }

    #[test]
    #[serial]
    fn env_overrides_respects_prefix() {
        // Clean possibly conflicting vars
        clear_env(&[
            "SERVER__SECURITY__REQUIRE_MAC",
            "AIRFRAME__SERVER__SECURITY__REQUIRE_MAC",
        ]);

        // Non-prefixed variable should be ignored
        std::env::set_var("SERVER__SECURITY__REQUIRE_MAC", "true");
        let mut dst = toml::Value::Table(Default::default());
        merge_from_env_with_prefixes(&mut dst, &["AIRFRAME__".to_string()]);
        // Expect no "server.security.require_mac" present
        assert!(dst.get("server").is_none() || dst["server"].get("security").is_none());

        // Now set the prefixed one and ensure it's applied
        std::env::set_var("AIRFRAME__SERVER__SECURITY__REQUIRE_MAC", "true");
        let mut dst2 = toml::Value::Table(Default::default());
        merge_from_env_with_prefixes(&mut dst2, &["AIRFRAME__".to_string()]);
        let val = dst2["server"].as_table().unwrap()["security"]
            .as_table()
            .unwrap()["require_mac"]
            .as_bool();
        assert_eq!(val, Some(true));

        // Cleanup
        clear_env(&[
            "SERVER__SECURITY__REQUIRE_MAC",
            "AIRFRAME__SERVER__SECURITY__REQUIRE_MAC",
        ]);
    }

    #[test]
    #[serial]
    fn env_overrides_multiple_prefixes_supported() {
        clear_env(&[
            "NANOKEY__SERVER__SECURITY__REQUIRE_MAC",
            "NANOPASS__SERVER__SECURITY__REQUIRE_MAC",
        ]);
        std::env::set_var("NANOKEY__SERVER__SECURITY__REQUIRE_MAC", "false");
        std::env::set_var("NANOPASS__SERVER__SECURITY__REQUIRE_MAC", "true");
        let mut dst = toml::Value::Table(Default::default());
        let prefixes = vec!["NANOKEY__".to_string(), "NANOPASS__".to_string()];
        merge_from_env_with_prefixes(&mut dst, &prefixes);
        // Both set the same key; last one wins only if env iteration orders them that way.
        // We do not guarantee inter-prefix precedence here; we only guarantee both are recognized.
        // So assert that the key exists and is either true or false depending on platform iteration order.
        let v = dst["server"].as_table().unwrap()["security"]
            .as_table()
            .unwrap()["require_mac"]
            .as_bool();
        assert!(v.is_some());

        // Cleanup
        clear_env(&[
            "NANOKEY__SERVER__SECURITY__REQUIRE_MAC",
            "NANOPASS__SERVER__SECURITY__REQUIRE_MAC",
        ]);
    }

    #[test]
    #[serial]
    fn env_overrides_types_are_parsed() {
        // Prepare env
        clear_env(&[
            "AIRFRAME__FLAGS__ENABLED",
            "AIRFRAME__X__N",
            "AIRFRAME__APP__NAME",
            "AIRFRAME__LOGGING__DIRECTIVES",
        ]);

        std::env::set_var("AIRFRAME__FLAGS__ENABLED", "true");
        std::env::set_var("AIRFRAME__X__N", "123");
        std::env::set_var("AIRFRAME__APP__NAME", "nanopass");
        std::env::set_var("AIRFRAME__LOGGING__DIRECTIVES", "[debug,info]");

        let mut dst = toml::Value::Table(Default::default());
        merge_from_env_with_prefixes(&mut dst, &["AIRFRAME__".to_string()]);

        assert_eq!(
            dst["flags"].as_table().unwrap()["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(dst["x"].as_table().unwrap()["n"].as_integer(), Some(123));
        assert_eq!(
            dst["app"].as_table().unwrap()["name"].as_str(),
            Some("nanopass")
        );

        let arr = dst["logging"].as_table().unwrap()["directives"]
            .as_array()
            .expect("array");
        let items: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(items, vec!["debug", "info"]);

        // Cleanup
        clear_env(&[
            "AIRFRAME__FLAGS__ENABLED",
            "AIRFRAME__X__N",
            "AIRFRAME__APP__NAME",
            "AIRFRAME__LOGGING__DIRECTIVES",
        ]);
    }
}
