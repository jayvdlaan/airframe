//! Core types for airframe_config.
//! During the refactor, these types are defined here to keep the crate layout clean.

use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BasicConfig {
    pub raw: toml::Value,
    pub source: Option<PathBuf>,
}

impl Default for BasicConfig {
    fn default() -> Self {
        Self {
            raw: toml::Value::Table(Default::default()),
            source: None,
        }
    }
}

impl BasicConfig {
    pub fn get<T: serde::de::DeserializeOwned + Default>(&self, section: &str) -> T {
        // Support dot-path lookup like "server.security" across nested tables.
        fn get_by_dot_path<'a>(v: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
            let mut cur = v;
            for part in path.split('.') {
                match cur {
                    toml::Value::Table(t) => {
                        cur = t.get(part)?;
                    }
                    _ => return None,
                }
            }
            Some(cur)
        }

        get_by_dot_path(&self.raw, section)
            .and_then(|v| v.clone().try_into().ok())
            .unwrap_or_default()
    }

    /// Creates a BasicConfig from dotted-path pairs, inserting string values under nested tables.
    /// Example: ("server.nanokey.base_url", "https://host") will produce
    /// { server = { nanokey = { base_url = "https://host" } } }
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        use crate::io::files::parse_scalar;
        use toml::value::Table;
        let mut root = toml::Value::Table(Table::new());
        for (path, value) in pairs {
            let parts: Vec<&str> = path.split('.').collect();
            // Navigate/build nested tables
            let mut cur = root.as_table_mut().expect("root must be table");
            for (i, part) in parts.iter().enumerate() {
                let is_last = i == parts.len() - 1;
                if is_last {
                    // Parse scalar types so booleans/ints/floats round-trip correctly
                    cur.insert((*part).to_string(), parse_scalar(value));
                } else {
                    if !cur.contains_key(*part) {
                        cur.insert((*part).to_string(), toml::Value::Table(Table::new()));
                    }
                    // safe to unwrap since we just ensured it's a table
                    cur = cur
                        .get_mut(*part)
                        .and_then(toml::Value::as_table_mut)
                        .expect("intermediate should be table");
                }
            }
        }
        Self {
            raw: root,
            source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BasicConfig;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct SecurityCfg {
        require_mac: bool,
    }

    #[test]
    fn basic_config_dot_path_and_typed_get() {
        // Build config with nested key via from_pairs
        let bc = BasicConfig::from_pairs(&[
            ("server.security.require_mac", "true"),
            ("server.name", "np"),
        ]);

        // Untyped: get a nested map as serde_json::Value
        let sec: serde_json::Value = bc.get("server.security");
        assert!(sec.is_object());
        assert_eq!(sec.get("require_mac").and_then(|v| v.as_bool()), Some(true));

        // Typed decode into our struct
        let typed: SecurityCfg = bc.get("server.security");
        assert_eq!(typed, SecurityCfg { require_mac: true });

        // Missing key: untyped should be Null; typed should be default()
        let missing_untyped: serde_json::Value = bc.get("server.missing");
        assert!(missing_untyped.is_null());
        #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
        struct Foo {
            a: i32,
        }
        let missing_typed: Foo = bc.get("does.not.exist");
        assert_eq!(missing_typed, Foo { a: 0 });
    }

    #[test]
    fn from_pairs_builds_nested() {
        let bc = BasicConfig::from_pairs(&[
            ("server.nanokey.base_url", "https://host"),
            ("logging.level", "info"),
        ]);

        // Check via get rehydration
        let nk: serde_json::Value = bc.get("server.nanokey");
        assert_eq!(
            nk.get("base_url").and_then(|v| v.as_str()),
            Some("https://host")
        );

        let lvl: String = bc.get("logging.level");
        assert_eq!(lvl, "info".to_string());
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ConfigReloaded;

#[cfg(feature = "module")]
impl airframe_core::bus::Event for ConfigReloaded {
    const NAME: &'static str = "ConfigReloaded";
}

/// Event published once the config file watcher has been successfully installed
/// and is ready to emit change notifications. Tests can wait on this to avoid
/// races between initial publish and watcher readiness.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ConfigWatcherReady;

#[cfg(feature = "module")]
impl airframe_core::bus::Event for ConfigWatcherReady {
    const NAME: &'static str = "ConfigWatcherReady";
}
