//! Logging configuration types.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct LoggingConfig {
    // New global directives that form the baseline EnvFilter, e.g., ["info", "my_crate::db=debug"]
    pub directives: Option<Vec<String>>,
    // Legacy: Global default level (e.g., "info", "debug")
    pub level: Option<String>,
    // Legacy: Optional full env filter string, e.g., "my_crate=debug,info"
    pub env_filter: Option<String>,
    // Legacy: Optional per-target levels, merged with `level` into an EnvFilter when `env_filter` is not provided
    pub targets: Option<std::collections::HashMap<String, String>>,
    // Legacy top-level console formatting options (applied if no sinks specified)
    pub json: Option<bool>,
    pub ansi: Option<bool>,
    // Global non-blocking buffer size (lines) for file sinks; default is implementation-defined.
    pub non_blocking_buffer_lines: Option<usize>,
    // New multi-sink configuration (if present, preferred over legacy json/ansi)
    pub sinks: Option<Vec<SinkConfig>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct FormatOptions {
    pub with_span_events: Option<String>,     // none|new|enter|full
    pub timestamp: Option<bool>,              // default true
    pub target: Option<bool>,                 // default true
    pub level: Option<bool>,                  // default true
    pub thread: Option<bool>,                 // default false
    pub file: Option<bool>,                   // default false
    pub line: Option<bool>,                   // default false
    pub pretty_json: Option<bool>,            // not supported; reserved
    pub include_correlation_id: Option<bool>, // default true; when true, include correlation_id if present
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RotationConfig {
    Policy(String),
    Size {
        policy: String,
        max_bytes: u64,
        keep: usize,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum SinkConfig {
    #[serde(rename = "console")]
    Console {
        json: Option<bool>,
        ansi: Option<bool>,
        filter: Option<String>,
        format: Option<FormatOptions>,
        stderr: Option<bool>,
    },
    // File sink; supports per-sink filter and rotation policy
    #[serde(rename = "file")]
    File {
        path: String,
        json: Option<bool>,
        ansi: Option<bool>,
        filter: Option<String>,
        rotation: Option<RotationConfig>,
        format: Option<FormatOptions>,
    },
    // Journald sink (Linux/systemd), only built when feature "journald" is enabled
    #[serde(rename = "journald")]
    Journald { filter: Option<String> },
    // Syslog sink (feature-gated). If the feature is not enabled, the entry is ignored at runtime.
    #[serde(rename = "syslog")]
    Syslog { filter: Option<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_config_default_is_empty() {
        let d = LoggingConfig::default();
        assert_eq!(d.directives, None);
        assert_eq!(d.level, None);
        assert_eq!(d.env_filter, None);
        assert_eq!(d.targets, None);
        assert_eq!(d.json, None);
        assert_eq!(d.ansi, None);
        assert_eq!(d.non_blocking_buffer_lines, None);
        assert_eq!(d.sinks, None);
    }

    #[test]
    fn sink_config_console_serde_roundtrip() {
        let s = SinkConfig::Console {
            json: Some(true),
            ansi: Some(false),
            filter: Some("my::target=debug".into()),
            format: None,
            stderr: Some(true),
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: SinkConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
        // Ensure tag is present
        assert!(j.contains("\"kind\":\"console\""));
    }

    #[test]
    fn sink_config_file_with_rotation_serde() {
        let r = RotationConfig::Size {
            policy: "size".into(),
            max_bytes: 1024,
            keep: 3,
        };
        let s = SinkConfig::File {
            path: "logs/app.log".into(),
            json: Some(false),
            ansi: Some(false),
            filter: Some("info".into()),
            rotation: Some(r.clone()),
            format: Some(FormatOptions::default()),
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: SinkConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
        // RotationConfig is embedded
        assert!(j.contains("max_bytes"));
        assert!(matches!(r, RotationConfig::Size { .. }));
    }

    #[test]
    fn rotation_config_untagged_variants() {
        // Policy form (untagged string)
        let j = "\"daily\"";
        let rc: RotationConfig = serde_json::from_str(j).unwrap();
        assert!(matches!(rc, RotationConfig::Policy(ref p) if p == "daily"));

        // Size form (object)
        let j = r#"{"policy":"size","max_bytes":10,"keep":2}"#;
        let rc: RotationConfig = serde_json::from_str(j).unwrap();
        match rc {
            RotationConfig::Size {
                policy,
                max_bytes,
                keep,
            } => {
                assert_eq!(policy, "size");
                assert_eq!(max_bytes, 10);
                assert_eq!(keep, 2);
            }
            _ => panic!("wrong variant"),
        }
    }
}
