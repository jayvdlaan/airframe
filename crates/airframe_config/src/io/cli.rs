//! CLI argument configuration sources.
//!
//! Implements CLI overrides and config path parsing.
// Some helpers may be unused when neither `module` nor `args` features are enabled.
#![cfg_attr(all(not(feature = "module"), not(feature = "args")), allow(dead_code))]
use crate::io::files::split_paths;
use crate::io::files::{parse_scalar, set_path};

pub(crate) fn merge_from_cli(dst: &mut toml::Value, args: &[String]) {
    for arg in args {
        if let Some(rest) = arg.strip_prefix("--cfg.") {
            if let Some(eq) = rest.find('=') {
                let (path, value) = rest.split_at(eq);
                let value = &value[1..];
                let parts: Vec<&str> = path.split('.').collect();
                set_path(dst, &parts, parse_scalar(value));
            }
        } else if let Some(rest) = arg.strip_prefix("--cfg=") {
            if let Some(eq) = rest.find('=') {
                let (path, value) = rest.split_at(eq);
                let value = &value[1..];
                let parts: Vec<&str> = path.split('.').collect();
                set_path(dst, &parts, parse_scalar(value));
            }
        }
    }
}

#[cfg(test)]
mod merge_from_cli_tests {
    use super::merge_from_cli;

    fn sv(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn both_forms_set_identical_nested_keys() {
        // Using --cfg.path.to.key=value
        let mut dst1 = toml::Value::Table(Default::default());
        merge_from_cli(&mut dst1, &sv(&["--cfg.server.security.require_mac=false"]));

        // Using --cfg=path.to.key=value
        let mut dst2 = toml::Value::Table(Default::default());
        merge_from_cli(&mut dst2, &sv(&["--cfg=server.security.require_mac=false"]));

        assert_eq!(dst1, dst2);

        // Ensure nested materialization
        let t = dst1.as_table().unwrap();
        let server = t
            .get("server")
            .and_then(|v| v.as_table())
            .expect("server table");
        let sec = server
            .get("security")
            .and_then(|v| v.as_table())
            .expect("security table");
        let mac = sec
            .get("require_mac")
            .and_then(|v| v.as_bool())
            .expect("bool value");
        assert!(!mac);
    }

    #[test]
    fn scalar_parsing_bool_int_float_and_string() {
        let mut dst = toml::Value::Table(Default::default());
        merge_from_cli(
            &mut dst,
            &sv(&[
                "--cfg.feature.enabled=true",
                "--cfg.service.retries=3",
                "--cfg.service.ratio=1.5",
                "--cfg.app.name=nanopass",
            ]),
        );

        let t = dst.as_table().unwrap();
        assert_eq!(
            t["feature"].as_table().unwrap()["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(
            t["service"].as_table().unwrap()["retries"].as_integer(),
            Some(3)
        );
        assert!(
            (t["service"].as_table().unwrap()["ratio"]
                .as_float()
                .unwrap()
                - 1.5)
                .abs()
                < 1e-9
        );
        assert_eq!(
            t["app"].as_table().unwrap()["name"].as_str(),
            Some("nanopass")
        );
    }

    #[test]
    fn list_parsing_into_array() {
        let mut dst = toml::Value::Table(Default::default());
        merge_from_cli(&mut dst, &sv(&["--cfg.logging.directives=[debug,info]"]));

        let arr = dst["logging"].as_table().unwrap()["directives"]
            .as_array()
            .expect("array");
        let items: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(items, vec!["debug", "info"]);
    }

    #[test]
    fn cli_overrides_scalar_and_list_values() {
        let mut dst = toml::Value::Table(Default::default());
        merge_from_cli(
            &mut dst,
            &sv(&[
                "--cfg.logging.directives=[debug,info]",
                "--cfg.x.n=123",
                "--cfg.flags.enabled=true",
                "--cfg.flags.disabled=false",
            ]),
        );

        // Array assertion
        let arr = dst["logging"].as_table().unwrap()["directives"]
            .as_array()
            .expect("array");
        let items: Vec<&str> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(items, vec!["debug", "info"]);

        // Integer assertion
        assert_eq!(dst["x"].as_table().unwrap()["n"].as_integer(), Some(123));

        // Boolean assertions
        assert_eq!(
            dst["flags"].as_table().unwrap()["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(
            dst["flags"].as_table().unwrap()["disabled"].as_bool(),
            Some(false)
        );
    }
}

pub(crate) fn config_paths_from_args(argv: &[String]) -> Option<Vec<std::path::PathBuf>> {
    // Accept forms: --config=foo, --config foo, --config-path=foo, --config-path foo
    let mut out: Option<Vec<std::path::PathBuf>> = None;
    let mut i = 0usize;
    while i < argv.len() {
        let a = &argv[i];
        let mut take_val = None;
        if let Some(val) = a.strip_prefix("--config=") {
            take_val = Some(val.to_string());
        } else if let Some(val) = a.strip_prefix("--config-path=") {
            take_val = Some(val.to_string());
        } else if (a == "--config" || a == "--config-path") && i + 1 < argv.len() {
            i += 1;
            take_val = Some(argv[i].clone());
        }
        if let Some(val) = take_val {
            out = Some(split_paths(&val));
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::config_paths_from_args;
    use std::path::PathBuf;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_config_eq() {
        let argv = s(&["--config=foo.toml"]);
        let paths = config_paths_from_args(&argv).expect("Some paths");
        assert_eq!(paths, vec![PathBuf::from("foo.toml")]);
    }

    #[test]
    fn parses_config_spaced() {
        let argv = s(&["--config", "foo.toml"]);
        let paths = config_paths_from_args(&argv).expect("Some paths");
        assert_eq!(paths, vec![PathBuf::from("foo.toml")]);
    }

    #[test]
    fn parses_config_path_eq() {
        let argv = s(&["--config-path=foo.toml"]);
        let paths = config_paths_from_args(&argv).expect("Some paths");
        assert_eq!(paths, vec![PathBuf::from("foo.toml")]);
    }

    #[test]
    fn parses_config_path_spaced() {
        let argv = s(&["--config-path", "foo.toml"]);
        let paths = config_paths_from_args(&argv).expect("Some paths");
        assert_eq!(paths, vec![PathBuf::from("foo.toml")]);
    }

    #[test]
    fn last_occurrence_wins() {
        // When multiple flags are present, the last occurrence should be used
        let argv = s(&["--config=first.toml", "--config-path", "second.toml"]);
        let paths = config_paths_from_args(&argv).expect("Some paths");
        assert_eq!(paths, vec![PathBuf::from("second.toml")]);

        let argv2 = s(&[
            "--config-path=one.toml",
            "--config",
            "two.toml",
            "--config=three.toml",
        ]);
        let paths2 = config_paths_from_args(&argv2).expect("Some paths");
        assert_eq!(paths2, vec![PathBuf::from("three.toml")]);
    }
}
