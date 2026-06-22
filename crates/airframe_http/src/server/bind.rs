//! server::bind — bind-address discovery and resolution.
//!
//! Resolves the HTTP server's listen address from (in precedence order)
//! CLI flags, environment variables, config, and a default. Extracted from
//! `axum_server.rs` as a pure move; behavior is identical.

use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;

// --- Helpers to discover bind from CLI/ENV/inline-config ---

/// Scan argv for a bind value using a broad set of flags and syntaxes.
/// Supported forms (in order of discovery within argv):
///   --bind HOST:PORT | --bind=HOST:PORT
///   --http.bind HOST:PORT | --http.bind=HOST:PORT
///   --server.bind HOST:PORT | --server.bind=HOST:PORT
///   --listen HOST:PORT | --listen=HOST:PORT
///   -b HOST:PORT | -bHOST:PORT
/// Returns (value, source_flag) when found.
fn scan_argv_for_bind(argv: &[String]) -> Option<(String, String)> {
    let long_flags = ["--bind", "--http.bind", "--server.bind", "--listen"];
    for (i, a) in argv.iter().enumerate() {
        let s = a.as_str();
        // Exact long flags expecting a following value
        if long_flags.contains(&s) {
            if i + 1 < argv.len() {
                return Some((argv[i + 1].clone(), s.to_string()));
            } else {
                return None;
            }
        }
        // Long flags in --flag=value form
        for lf in &long_flags {
            let prefix = format!("{}=", lf);
            if s.starts_with(&prefix) {
                return Some((s[prefix.len()..].to_string(), lf.to_string()));
            }
        }
        // Short -b value (next token) or -bHOST:PORT compact
        if s == "-b" {
            if i + 1 < argv.len() {
                return Some((argv[i + 1].clone(), "-b".to_string()));
            } else {
                return None;
            }
        }
        if s.starts_with("-b") && s.len() > 2 {
            return Some((s[2..].to_string(), "-b".to_string()));
        }
    }
    None
}

/// Build the precedence-ordered environment variable list and return first valid SocketAddr.
/// Order: <APP>_BIND (derived) -> NANOPASS_BIND -> NANOKEY_BIND -> AFRAME_HTTP_BIND -> HTTP_BIND
fn resolve_bind_from_env() -> Option<(SocketAddr, String)> {
    let mut keys: Vec<String> = Vec::new();
    // Derive APP name from argv[0]
    if let Some(argv0) = std::env::args().next() {
        use std::path::Path;
        if let Some(stem) = Path::new(&argv0).file_stem().and_then(|s| s.to_str()) {
            if !stem.is_empty() {
                let mut derived = String::new();
                for ch in stem.chars() {
                    if ch.is_ascii_alphanumeric() {
                        derived.push(ch.to_ascii_uppercase());
                    } else {
                        derived.push('_');
                    }
                }
                if !derived.is_empty() {
                    keys.push(format!("{}_BIND", derived));
                }
            }
        }
    }
    keys.push("NANOPASS_BIND".to_string());
    keys.push("NANOKEY_BIND".to_string());
    keys.push("AFRAME_HTTP_BIND".to_string());
    keys.push("HTTP_BIND".to_string());
    // de-dup
    let mut seen = std::collections::HashSet::new();
    keys.retain(|k| seen.insert(k.clone()));
    for k in keys {
        if let Ok(v) = std::env::var(&k) {
            if let Ok(addr) = SocketAddr::from_str(&v) {
                return Some((addr, format!("env:{}", k)));
            }
        }
    }
    None
}

/// Scan argv for a config path flag.
/// Supported forms:
///   --config-path PATH | --config-path=PATH
///   --config PATH | --config=PATH
///   -c PATH
fn scan_argv_for_config_path(argv: &[String]) -> Option<(String, String)> {
    let long_flags = ["--config-path", "--config"];
    for (i, a) in argv.iter().enumerate() {
        let s = a.as_str();
        if long_flags.contains(&s) {
            if i + 1 < argv.len() {
                return Some((argv[i + 1].clone(), s.to_string()));
            } else {
                return None;
            }
        }
        for lf in &long_flags {
            let prefix = format!("{}=", lf);
            if s.starts_with(&prefix) {
                return Some((s[prefix.len()..].to_string(), lf.to_string()));
            }
        }
        if s == "-c" {
            if i + 1 < argv.len() {
                return Some((argv[i + 1].clone(), "-c".to_string()));
            } else {
                return None;
            }
        }
    }
    None
}

/// Attempt to parse a bind from a JSON string value tree
fn try_parse_bind_from_json(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v
        .get("server")
        .and_then(|t| t.get("bind"))
        .and_then(|b| b.as_str())
    {
        return Some(s.to_string());
    }
    if let Some(s) = v
        .get("http")
        .and_then(|t| t.get("bind"))
        .and_then(|b| b.as_str())
    {
        return Some(s.to_string());
    }
    if let Some(s) = v.get("server.bind").and_then(|b| b.as_str()) {
        return Some(s.to_string());
    }
    None
}

/// Attempt to parse a bind from a TOML document (enabled when feature inline_config_parse is on)
#[cfg(feature = "inline_config_parse")]
fn try_parse_bind_from_toml_str(s: &str) -> Option<String> {
    let v: toml::Value = toml::from_str(s).ok()?;
    if let Some(server) = v.get("server") {
        if let Some(b) = server.get("bind").and_then(|x| x.as_str()) {
            return Some(b.to_string());
        }
    }
    if let Some(http) = v.get("http") {
        if let Some(b) = http.get("bind").and_then(|x| x.as_str()) {
            return Some(b.to_string());
        }
    }
    None
}

/// Fallback: attempt to load config file and extract bind without requiring airframe_config
fn resolve_bind_from_inline_config(argv: &[String]) -> Option<(SocketAddr, String)> {
    let (path, _src_flag) = scan_argv_for_config_path(argv)?;
    let p = Path::new(&path);
    let contents = std::fs::read_to_string(p).ok()?;
    // Try TOML first if feature is enabled, then JSON
    #[cfg(feature = "inline_config_parse")]
    {
        if let Some(bind) = try_parse_bind_from_toml_str(&contents) {
            if let Ok(addr) = SocketAddr::from_str(&bind) {
                let src = format!("config:file:{}:toml", p.display());
                return Some((addr, src));
            }
        }
    }
    // Try JSON
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
        if let Some(bind) = try_parse_bind_from_json(&json) {
            if let Ok(addr) = SocketAddr::from_str(&bind) {
                let src = format!("config:file:{}:json", p.display());
                return Some((addr, src));
            }
        }
    }
    None
}

/// Resolve bind address using precedence: CLI > ENV > CONFIG > DEFAULT
/// Returns (addr, source)
#[allow(unused_variables)]
#[cfg(feature = "module")]
pub(super) fn resolve_bind_addr(
    default: SocketAddr,
    ctx: Option<&airframe_core::module::ModuleContext>,
) -> (SocketAddr, String) {
    // 1) CLI: scan raw argv for common flags (works with or without ArgsModule)
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if let Some((v, src)) = scan_argv_for_bind(&raw) {
        if let Ok(addr) = SocketAddr::from_str(&v) {
            return (addr, format!("cli:{}", src));
        }
    }

    // 2) Environment variables
    if let Some((addr, src)) = resolve_bind_from_env() {
        return (addr, src);
    }

    // 3) Config via airframe_config BasicConfig, if feature and service available
    #[cfg(feature = "config")]
    if let Some(ctx) = ctx {
        if let Some(basic) = ctx
            .services
            .get::<airframe_config::api::types::BasicConfig>()
        {
            // Try a few common paths
            for path in ["server.bind", "http.bind", "server"] {
                let v: serde_json::Value = basic.get(path);
                if let Some(s) = v.as_str() {
                    if let Ok(addr) = SocketAddr::from_str(s) {
                        return (addr, format!("config:{}", path));
                    }
                } else if let Some(obj) = v.as_object() {
                    if let Some(sv) = obj.get("bind").and_then(|vv| vv.as_str()) {
                        if let Ok(addr) = SocketAddr::from_str(sv) {
                            return (addr, format!("config:{}:bind", path));
                        }
                    }
                }
            }
        }
    }

    // 3b) Inline config parse fallback (works even without airframe_config)
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if let Some((addr, src)) = resolve_bind_from_inline_config(&raw) {
        return (addr, src);
    }

    // 4) Default
    (default, "default".to_string())
}

#[allow(unused_variables)]
#[cfg(not(feature = "module"))]
pub(super) fn resolve_bind_addr(default: SocketAddr) -> (SocketAddr, String) {
    // 1) CLI: scan raw argv for common flags (works with or without ArgsModule)
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if let Some((v, src)) = scan_argv_for_bind(&raw) {
        if let Ok(addr) = SocketAddr::from_str(&v) {
            return (addr, format!("cli:{}", src));
        }
    }

    // 2) Environment variables
    if let Some((addr, src)) = resolve_bind_from_env() {
        return (addr, src);
    }

    // 3) No access to airframe_config BasicConfig without ModuleContext; skip.

    // 3b) Inline config parse fallback (works even without airframe_config)
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if let Some((addr, src)) = resolve_bind_from_inline_config(&raw) {
        return (addr, src);
    }

    // 4) Default
    (default, "default".to_string())
}
