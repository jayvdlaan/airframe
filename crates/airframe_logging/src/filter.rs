use crate::api::config::LoggingConfig;

pub(crate) fn build_env_filter_from_config(cfg: &LoggingConfig) -> tracing_subscriber::EnvFilter {
    // Precedence: RUST_LOG env var (if set) > explicit env_filter > directives > legacy targets/level > default info
    if let Ok(from_env) = tracing_subscriber::EnvFilter::try_from_default_env() {
        return from_env;
    }
    // Then honor config
    if let Some(ref ef) = cfg.env_filter {
        tracing_subscriber::EnvFilter::new(ef.clone())
    } else if let Some(ref dirs) = cfg.directives {
        let spec = dirs.join(",");
        tracing_subscriber::EnvFilter::new(spec)
    } else {
        // Merge default level and per-target directives (legacy path)
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref targets) = cfg.targets {
            // Append target directives first; default level last so precedence follows left-to-right first match
            for (t, lvl) in targets.iter() {
                parts.push(format!("{}={}", t, lvl));
            }
        }
        let default_level = cfg.level.clone().unwrap_or_else(|| "info".to_string());
        parts.push(default_level);
        let spec = parts.join(",");
        tracing_subscriber::EnvFilter::new(spec)
    }
}
