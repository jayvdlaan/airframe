use crate::api::config::LoggingConfig;

pub(crate) fn validate_sinks_first_schema(cfg: &LoggingConfig) -> anyhow::Result<()> {
    // Reject legacy top-level keys: level, env_filter, targets, json, ansi
    if cfg.level.is_some()
        || cfg.env_filter.is_some()
        || cfg.targets.as_ref().map(|m| !m.is_empty()).unwrap_or(false)
        || cfg.json.is_some()
        || cfg.ansi.is_some()
    {
        anyhow::bail!("Legacy logging keys are no longer supported. Use the sinks-first schema: [logging].directives and [[logging.sinks]] with per-sink options.");
    }

    // Mobile policy: syslog is not a meaningful/available sink on Android/iOS.
    // Keep the module supported, but reject unsupported sink configs early and clearly.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if cfg.sinks.as_ref().is_some_and(|sinks| {
            sinks
                .iter()
                .any(|s| matches!(s, crate::api::config::SinkConfig::Syslog { .. }))
        }) {
            anyhow::bail!(
                "syslog sink is not supported on mobile targets; use console/file (or a mobile-native sink)"
            );
        }
    }

    // Platform policy: journald is Linux/systemd-specific.
    // Keep the module supported, but reject a journald sink configuration on non-Linux targets.
    #[cfg(not(target_os = "linux"))]
    {
        if cfg.sinks.as_ref().is_some_and(|sinks| {
            sinks
                .iter()
                .any(|s| matches!(s, crate::api::config::SinkConfig::Journald { .. }))
        }) {
            anyhow::bail!(
                "journald sink is only supported on linux targets; use console/file/syslog (or remove journald sink)"
            );
        }
    }

    // Prefer sinks-first: encourage defining sinks explicitly, but don't hard error if none are present.
    // If no sinks are configured, the runtime will proceed (e.g., for tests) and may install defaults elsewhere.
    Ok(())
}

#[cfg(test)]
mod tests_platform_sink_validation {
    use crate::api::config::{LoggingConfig, SinkConfig};
    use crate::validation::validate_sinks_first_schema;

    #[test]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn syslog_sink_is_allowed_on_non_mobile_targets() {
        let cfg = LoggingConfig {
            sinks: Some(vec![SinkConfig::Syslog { filter: None }]),
            ..Default::default()
        };
        validate_sinks_first_schema(&cfg).expect("syslog sink should be allowed on non-mobile");
    }

    #[test]
    #[cfg(any(target_os = "android", target_os = "ios"))]
    fn syslog_sink_is_rejected_on_mobile_targets() {
        let cfg = LoggingConfig {
            sinks: Some(vec![SinkConfig::Syslog { filter: None }]),
            ..Default::default()
        };
        let err = validate_sinks_first_schema(&cfg)
            .err()
            .expect("expected syslog sink to be rejected on mobile");
        let msg = err.to_string();
        assert!(msg.to_ascii_lowercase().contains("syslog"));
        assert!(msg.to_ascii_lowercase().contains("mobile"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn journald_sink_is_allowed_on_linux_targets() {
        let cfg = LoggingConfig {
            sinks: Some(vec![SinkConfig::Journald { filter: None }]),
            ..Default::default()
        };
        validate_sinks_first_schema(&cfg).expect("journald sink should be allowed on linux");
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn journald_sink_is_rejected_on_non_linux_targets() {
        let cfg = LoggingConfig {
            sinks: Some(vec![SinkConfig::Journald { filter: None }]),
            ..Default::default()
        };
        let err = validate_sinks_first_schema(&cfg)
            .err()
            .expect("expected journald sink to be rejected on non-linux");
        let msg = err.to_string().to_ascii_lowercase();
        assert!(msg.contains("journald"));
        assert!(msg.contains("linux"));
    }
}
