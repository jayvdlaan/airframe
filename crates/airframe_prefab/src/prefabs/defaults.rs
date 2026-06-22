//! Default configuration packs (TOML) for each prefab.
//! These values are the lowest precedence layer and can be overridden by files/env/CLI.

/// Runtime profile for prefab defaults.
/// Use Dev during development, Test in automated tests, and Prod for production.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PrefabProfile {
    Dev,
    Test,
    Prod,
}

/// Profile defaults: low‑precedence tweaks commonly desirable per profile.
/// These are designed to layer underneath config files/env/CLI and underneath
/// prefab base defaults. Later layers can override.
///
/// Deterministic behavior: keys returned here will override earlier contributor
/// layers but will be overridden by files/env/CLI.
pub fn profile_defaults(profile: PrefabProfile) -> toml::Value {
    match profile {
        PrefabProfile::Dev => toml::toml! {
            [logging]
            directives = ["debug", "hyper=info"]

            [shutdown]
            grace_period = "5s"

            [cors]
            enable = true

            [gateway]
            connect_timeout_ms = 200
            request_timeout_ms = 1000

            [admin]
            enable = true
            mutating_enable = true
        }
        .into(),
        PrefabProfile::Test => toml::toml! {
            [logging]
            directives = ["info"]

            [shutdown]
            grace_period = "1s"

            [gateway]
            connect_timeout_ms = 100
            request_timeout_ms = 500
        }
        .into(),
        PrefabProfile::Prod => toml::toml! {
            [logging]
            directives = ["info"]

            [admin]
            enable = true
            mutating_enable = false
        }
        .into(),
    }
}

/// Simple TOML merge used locally by prefab constructors to compose base defaults
/// with profile tweaks. Arrays are overwritten, tables are merged recursively, and
/// scalar values are overwritten by `src`.
pub fn merge_toml(dst: &mut toml::Value, src: toml::Value) {
    use toml::Value::*;
    match (dst, src) {
        (Table(dst_tbl), Table(src_tbl)) => {
            for (k, v) in src_tbl.into_iter() {
                match dst_tbl.get_mut(&k) {
                    Some(existing) => merge_toml(existing, v),
                    None => {
                        dst_tbl.insert(k, v);
                    }
                }
            }
        }
        (Array(dst_arr), Array(mut src_arr)) => {
            *dst_arr = std::mem::take(&mut src_arr);
        }
        (dst_slot, other) => {
            *dst_slot = other;
        }
    }
}

pub fn cli() -> toml::Value {
    toml::toml! {
        [logging]
        directives = ["info"]

        [[logging.sinks]]
        kind = "console"
        json = false
        ansi = true
        stderr = true
    }
    .into()
}

pub fn service() -> toml::Value {
    toml::toml! {
        [logging]
        directives = ["info"]

        [[logging.sinks]]
        kind = "console"
        json = false
        ansi = true
        stderr = true

        [admin]
        enable = true
        mutating_enable = false

        [server]
        bind = "127.0.0.1:8080"

        [shutdown]
        grace_period = "30s"

        [metrics]
        enable = false
    }
    .into()
}

pub fn http_api() -> toml::Value {
    toml::toml! {
        [logging]
        directives = ["info"]

        [[logging.sinks]]
        kind = "console"
        json = false
        ansi = true
        stderr = true

        [server]
        bind = "127.0.0.1:8080"

        [cors]
        enable = false
        allow_methods = ["GET", "POST", "OPTIONS"]
        allow_headers = ["Content-Type"]
        max_age = 600

        [admin]
        enable = true
        mutating_enable = false

        [metrics]
        enable = false
    }
    .into()
}

pub fn worker() -> toml::Value {
    toml::toml! {
        [logging]
        directives = ["info"]

        [[logging.sinks]]
        kind = "console"
        json = false
        ansi = true
        stderr = true

        [worker]
        max_attempts = 3
        base_backoff_ms = 100
        max_jitter_ms = 50

        [metrics]
        enable = false
    }
    .into()
}

pub fn gateway() -> toml::Value {
    toml::toml! {
        [logging]
        directives = ["info"]

        [[logging.sinks]]
        kind = "console"
        json = false
        ansi = true
        stderr = true

        [server]
        bind = "127.0.0.1:8080"

        [gateway]
        routes = []
        connect_timeout_ms = 1000
        request_timeout_ms = 5000
        max_body_bytes = 2097152
        streaming = false
        zero_copy_http = false

        [cors]
        enable = false

        [metrics]
        enable = false
    }
    .into()
}

pub fn scheduled() -> toml::Value {
    toml::toml! {
        [logging]
        directives = ["info"]

        [[logging.sinks]]
        kind = "console"
        json = false
        ansi = true
        stderr = true

        [scheduler]
        timezone = "UTC"
        leader_election = false
        jobs = []

        [metrics]
        enable = false
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_logging_defaults(v: &toml::Value) {
        // Ensure [logging] with directives ["info"] and a console sink to stderr
        let logging = v.get("logging").expect("logging table");
        let directives = logging.get("directives").expect("directives");
        let arr = directives.as_array().expect("directives array");
        assert!(arr.iter().any(|s| s.as_str() == Some("info")));
        let sinks = logging
            .get("sinks")
            .and_then(|s| s.as_array())
            .expect("sinks array");
        assert!(!sinks.is_empty());
        let first = sinks.first().unwrap();
        assert_eq!(first.get("kind").and_then(|k| k.as_str()), Some("console"));
        assert_eq!(first.get("stderr").and_then(|b| b.as_bool()), Some(true));
    }

    #[test]
    fn cli_defaults_have_expected_logging() {
        assert_logging_defaults(&cli());
    }

    #[test]
    fn service_defaults_have_expected_logging_and_admin() {
        let v = service();
        assert_logging_defaults(&v);
        assert_eq!(
            v.get("admin")
                .and_then(|t| t.get("enable"))
                .and_then(|b| b.as_bool()),
            Some(true)
        );
        assert_eq!(
            v.get("admin")
                .and_then(|t| t.get("mutating_enable"))
                .and_then(|b| b.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn http_api_defaults_have_expected_logging_and_server() {
        let v = http_api();
        assert_logging_defaults(&v);
        assert!(v
            .get("server")
            .and_then(|t| t.get("bind"))
            .and_then(|s| s.as_str())
            .is_some());
    }

    #[test]
    fn worker_defaults_have_expected_logging_and_worker_section() {
        let v = worker();
        assert_logging_defaults(&v);
        assert!(v.get("worker").is_some());
    }

    #[test]
    fn gateway_defaults_have_expected_logging_and_gateway_section() {
        let v = gateway();
        assert_logging_defaults(&v);
        assert!(v.get("gateway").is_some());
    }

    #[test]
    fn scheduled_defaults_have_expected_logging_and_scheduler_section() {
        let v = scheduled();
        assert_logging_defaults(&v);
        assert!(v.get("scheduler").is_some());
    }
}
