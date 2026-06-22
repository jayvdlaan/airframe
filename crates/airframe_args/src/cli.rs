//! Minimal CLI parsing helpers to support Pattern A across modules.
//! These helpers accept both `--flag=value` and `--flag value` forms.

/// Return the string value for a long option if present.
/// Examples: `--name=value` or `--name value`.
pub fn get_value(argv: &[String], long: &str) -> Option<String> {
    let mut i = 0usize;
    while i < argv.len() {
        let a = &argv[i];
        if let Some(v) = a.strip_prefix(&format!("{}=", long)) {
            return Some(v.to_string());
        }
        if a == long {
            if i + 1 < argv.len() {
                return Some(argv[i + 1].clone());
            }
            return None;
        }
        i += 1;
    }
    None
}

/// Return a boolean for a long flag if present.
/// Accepts `--flag` (true), `--flag=true/false`, and `--flag true/false`.
pub fn get_bool(argv: &[String], long: &str) -> Option<bool> {
    let mut i = 0usize;
    while i < argv.len() {
        let a = &argv[i];
        if let Some(v) = a.strip_prefix(&format!("{}=", long)) {
            return Some(!matches!(
                v.to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "off"
            ));
        }
        if a == long {
            // If a value follows and is not another flag, parse it; otherwise treat presence as true
            if i + 1 < argv.len() {
                let nxt = &argv[i + 1];
                if !nxt.starts_with('-') {
                    let v = nxt.to_ascii_lowercase();
                    return Some(!matches!(v.as_str(), "false" | "0" | "no" | "off"));
                }
            }
            return Some(true);
        }
        i += 1;
    }
    None
}

/// OS-portable split for lists of paths.
/// On Windows, splits on `,` or `;`. On Unix, also accepts `:`.
pub fn split_paths_os_portable(s: &str) -> Vec<std::path::PathBuf> {
    #[cfg(windows)]
    let seps: &[char] = &[',', ';'];
    #[cfg(not(windows))]
    let seps: &[char] = &[',', ';', ':'];
    s.split(|c| seps.contains(&c))
        .filter(|p| !p.is_empty())
        .map(std::path::PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_forms() {
        let argv = vec![
            "bin".to_string(),
            "--name=alice".to_string(),
            "--x".to_string(),
            "42".to_string(),
        ];
        assert_eq!(get_value(&argv, "--name").as_deref(), Some("alice"));
        assert_eq!(get_value(&argv, "--x").as_deref(), Some("42"));
        assert_eq!(get_value(&argv, "--missing"), None);
    }

    #[test]
    fn bool_forms() {
        let argv = vec![
            "bin".to_string(),
            "--flag".to_string(),
            "--nope=false".to_string(),
            "--x".to_string(),
            "off".to_string(),
        ];
        assert_eq!(get_bool(&argv, "--flag"), Some(true));
        assert_eq!(get_bool(&argv, "--nope"), Some(false));
        assert_eq!(get_bool(&argv, "--x"), Some(false));
        assert_eq!(get_bool(&argv, "--missing"), None);
    }

    #[test]
    #[cfg(windows)]
    fn split_paths_windows() {
        let s = r"C:\\a\\c.toml;D:\\b\\d.toml";
        let v = split_paths_os_portable(s);
        assert_eq!(v.len(), 2);
    }

    #[test]
    #[cfg(unix)]
    fn split_paths_unix() {
        let s = "/etc/a.toml:/etc/b.toml";
        let v = split_paths_os_portable(s);
        assert_eq!(v.len(), 2);
    }
}
