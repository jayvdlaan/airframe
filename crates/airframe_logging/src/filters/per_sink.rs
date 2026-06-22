//! Per-sink filter helpers.

#[derive(Clone, Debug)]
pub struct PerSinkFilter {
    pub(crate) default: Option<tracing::Level>,
    pub(crate) directives: Vec<(String, tracing::Level)>,
}

impl PerSinkFilter {
    pub fn parse(spec: &str) -> Self {
        let mut default = None;
        let mut directives = Vec::new();
        for part in spec.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if let Some((t, lvl)) = part.split_once('=') {
                if let Some(l) = parse_level(lvl.trim()) {
                    directives.push((t.trim().to_string(), l));
                }
            } else if let Some(l) = parse_level(part) {
                default = Some(l);
            }
        }
        PerSinkFilter {
            default,
            directives,
        }
    }
    pub fn allows(&self, meta: &tracing::Metadata<'_>) -> bool {
        let level_ok =
            |allowed: tracing::Level| level_to_num(meta.level()) <= level_to_num(&allowed);
        for (prefix, lvl) in &self.directives {
            if meta.target().starts_with(prefix) {
                return level_ok(*lvl);
            }
        }
        if let Some(def) = self.default {
            level_ok(def)
        } else {
            false
        }
    }
}

pub fn parse_level(s: &str) -> Option<tracing::Level> {
    match s.to_ascii_lowercase().as_str() {
        "error" => Some(tracing::Level::ERROR),
        "warn" | "warning" => Some(tracing::Level::WARN),
        "info" => Some(tracing::Level::INFO),
        "debug" => Some(tracing::Level::DEBUG),
        "trace" => Some(tracing::Level::TRACE),
        _ => None,
    }
}
pub fn level_to_num(l: &tracing::Level) -> u8 {
    match *l {
        tracing::Level::ERROR => 1,
        tracing::Level::WARN => 2,
        tracing::Level::INFO => 3,
        tracing::Level::DEBUG => 4,
        tracing::Level::TRACE => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_level_variants_and_unknown() {
        assert_eq!(parse_level("error"), Some(tracing::Level::ERROR));
        assert_eq!(parse_level("WARNING"), Some(tracing::Level::WARN));
        assert_eq!(parse_level("Info"), Some(tracing::Level::INFO));
        assert_eq!(parse_level("DeBuG"), Some(tracing::Level::DEBUG));
        assert_eq!(parse_level("trace"), Some(tracing::Level::TRACE));
        assert_eq!(parse_level("garbage"), None);
    }

    #[test]
    fn level_to_num_ordering() {
        assert!(level_to_num(&tracing::Level::ERROR) < level_to_num(&tracing::Level::WARN));
        assert!(level_to_num(&tracing::Level::WARN) < level_to_num(&tracing::Level::INFO));
        assert!(level_to_num(&tracing::Level::INFO) < level_to_num(&tracing::Level::DEBUG));
        assert!(level_to_num(&tracing::Level::DEBUG) < level_to_num(&tracing::Level::TRACE));
    }

    #[test]
    fn parse_filter_default_and_directives() {
        let f = PerSinkFilter::parse("info,my_crate=debug,other=warn");
        assert_eq!(f.default, Some(tracing::Level::INFO));
        assert_eq!(f.directives.len(), 2);
        assert!(f
            .directives
            .iter()
            .any(|(t, l)| t == "my_crate" && *l == tracing::Level::DEBUG));
        assert!(f
            .directives
            .iter()
            .any(|(t, l)| t == "other" && *l == tracing::Level::WARN));
    }

    #[test]
    fn allows_checks_prefix_and_default() {
        let f = PerSinkFilter::parse("info,my_crate=warn");

        // Build a temporary subscriber so we can use tracing metadata helpers
        let _guard = crate::testing::init_for_test("trace", false);

        // my_crate target at INFO should be rejected (needs WARN)
        let span_info = tracing::span!(target: "my_crate::db", tracing::Level::INFO, "ev");
        let meta_info = span_info.metadata().expect("metadata");
        assert!(!f.allows(meta_info));

        // my_crate target at WARN should pass
        let span_warn = tracing::span!(target: "my_crate::db", tracing::Level::WARN, "ev");
        let meta_warn = span_warn.metadata().expect("metadata");
        assert!(f.allows(meta_warn));

        // other target should use default (info): INFO passes, DEBUG does not
        let span_other_info = tracing::span!(target: "other", tracing::Level::INFO, "ev");
        let meta_other_info = span_other_info.metadata().expect("metadata");
        assert!(f.allows(meta_other_info));

        let span_other_debug = tracing::span!(target: "other", tracing::Level::DEBUG, "ev");
        let meta_other_debug = span_other_debug.metadata().expect("metadata");
        assert!(!f.allows(meta_other_debug));
    }

    #[test]
    fn empty_spec_disallows_all() {
        let f = PerSinkFilter::parse("");
        // Initialize a basic subscriber so spans have metadata
        let _guard = crate::testing::init_for_test("trace", false);
        // Without a default, only explicit directives would allow; meta should be rejected
        let span_err = tracing::span!(target: "any", tracing::Level::ERROR, "ev");
        let meta = span_err.metadata().expect("metadata");
        assert!(!f.allows(meta));
    }
}
