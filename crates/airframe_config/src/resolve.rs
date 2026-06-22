//! Configuration resolution and precedence logic.
//!
//! Host helpers to determine which config files to load, with clear precedence.

// When the `module` feature is disabled, these helpers may be unused in some builds.
#![cfg_attr(not(feature = "module"), allow(dead_code))]

/// Resolve the list of configuration file paths to load, based on (in order of precedence):
/// - Explicit CLI paths if provided (wins over env/default)
/// - AIRFRAME_CONFIG_PATH environment variable (colon/semicolon separated)
/// - A default path provided by the module, if any
///
/// Returns an ordered Vec where earlier items will be merged first (later wins).
pub(crate) fn resolve_paths(
    cli_paths: Option<Vec<std::path::PathBuf>>,
    env_path: Option<String>,
    default_path: Option<std::path::PathBuf>,
) -> Vec<std::path::PathBuf> {
    // If CLI explicitly provided paths, they take full precedence and exclude env/default.
    if let Some(cli) = cli_paths {
        return cli;
    }

    // Otherwise, layer in increasing precedence: default < env
    // Files are merged in the given order (later wins).
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    if let Some(p) = default_path {
        out.push(p);
    }
    if let Some(ep) = env_path {
        let mut env_paths = crate::io::files::split_paths(&ep);
        out.append(&mut env_paths);
    }
    out
}
