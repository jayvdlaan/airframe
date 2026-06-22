//! CLI override helpers.

#[cfg(feature = "args")]
pub fn apply_cli_overrides(cfg: &mut crate::api::config::LoggingConfig, argv: &[String]) {
    let mut i = 0usize;
    let mut out_spec: Option<String> = None;
    while i < argv.len() {
        let a = &argv[i];
        let take = |names: &[&str], i: &mut usize| -> Option<String> {
            for n in names {
                if let Some(v) = a.strip_prefix(&format!("{}=", n)) {
                    return Some(v.to_string());
                }
                if a == n && *i + 1 < argv.len() {
                    *i += 1;
                    return Some(argv[*i].clone());
                }
            }
            None
        };
        if let Some(v) = take(&["--log-filter"], &mut i) {
            cfg.env_filter = Some(v);
            cfg.level = None;
        }
        if let Some(v) = take(&["--log-level"], &mut i) {
            cfg.level = Some(v);
            cfg.env_filter = None;
        }
        if a == "--log-json" {
            ensure_console_sink(cfg, Some(true));
        }
        if let Some(v) = take(&["--log-json"], &mut i) {
            let b = v.parse::<bool>().unwrap_or(true);
            ensure_console_sink(cfg, Some(b));
        }
        if let Some(v) = take(&["--log-output"], &mut i) {
            out_spec = Some(v);
        }
        i += 1;
    }
    if let Some(spec) = out_spec {
        if let Some(sink) = parse_output_spec(&spec) {
            cfg.sinks = Some(vec![sink]); /* prefer sinks-first */
            cfg.json = None;
            cfg.ansi = None;
        }
    }
}

#[cfg(feature = "args")]
pub fn ensure_console_sink(cfg: &mut crate::api::config::LoggingConfig, json: Option<bool>) {
    use crate::api::config::SinkConfig;
    if let Some(ref mut sinks) = cfg.sinks {
        if sinks.is_empty() {
            sinks.push(SinkConfig::Console {
                json,
                ansi: None,
                filter: None,
                format: None,
                stderr: None,
            });
        }
    } else {
        cfg.sinks = Some(vec![SinkConfig::Console {
            json,
            ansi: None,
            filter: None,
            format: None,
            stderr: None,
        }]);
    }
}

#[cfg(feature = "args")]
pub fn parse_output_spec(s: &str) -> Option<crate::api::config::SinkConfig> {
    use crate::api::config::{RotationConfig, SinkConfig};
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("stdout") {
        return Some(SinkConfig::Console {
            json: None,
            ansi: None,
            filter: None,
            format: None,
            stderr: Some(false),
        });
    }
    if s.eq_ignore_ascii_case("stderr") {
        return Some(SinkConfig::Console {
            json: None,
            ansi: None,
            filter: None,
            format: None,
            stderr: Some(true),
        });
    }
    if let Some(rest) = s.strip_prefix("file:") {
        return Some(SinkConfig::File {
            path: rest.to_string(),
            json: None,
            ansi: None,
            filter: None,
            rotation: None,
            format: None,
        });
    }
    if let Some(rest) = s.strip_prefix("roll:size:") {
        let parts: Vec<&str> = rest.split(':').collect();
        let path = parts.first().map(|v| v.to_string()).unwrap_or_default();
        let max = parts
            .get(1)
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(10 * 1024 * 1024);
        let keep = parts
            .get(2)
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(5);
        return Some(SinkConfig::File {
            path,
            json: None,
            ansi: None,
            filter: None,
            rotation: Some(RotationConfig::Size {
                policy: "size".into(),
                max_bytes: max,
                keep,
            }),
            format: None,
        });
    }
    if let Some(rest) = s.strip_prefix("roll:daily:") {
        let mut it = rest.split(':');
        let dir = it.next().unwrap_or(".");
        let name = it.next().unwrap_or("app.log");
        return Some(SinkConfig::File {
            path: format!("{}/{}", dir, name),
            json: None,
            ansi: None,
            filter: None,
            rotation: Some(RotationConfig::Policy("daily".into())),
            format: None,
        });
    }
    if let Some(rest) = s.strip_prefix("roll:hourly:") {
        let mut it = rest.split(':');
        let dir = it.next().unwrap_or(".");
        let name = it.next().unwrap_or("app.log");
        return Some(SinkConfig::File {
            path: format!("{}/{}", dir, name),
            json: None,
            ansi: None,
            filter: None,
            rotation: Some(RotationConfig::Policy("hourly".into())),
            format: None,
        });
    }
    if s.eq_ignore_ascii_case("journald") {
        return Some(SinkConfig::Journald { filter: None });
    }
    if s.eq_ignore_ascii_case("syslog") {
        return Some(SinkConfig::Syslog { filter: None });
    }
    None
}
