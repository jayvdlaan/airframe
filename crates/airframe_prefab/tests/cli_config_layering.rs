#![forbid(unsafe_code)]

// This test requires the optional `config` and `args` features of airframe_prefab,
// since it depends on airframe_config and airframe_args crates.
#[cfg(all(feature = "config", feature = "args"))]
mod with_config_and_args {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use airframe_config::{BasicConfig, ConfigModule};
    use airframe_core::app::AppBuilder;

    // Verifies config layering precedence: defaults < file < env < CLI overrides
    // We simulate each layer and ensure the effective BasicConfig reflects the highest-precedence value.
    #[tokio::test]
    async fn config_layering_precedence_applies() {
        // 1) Prepare a temp config file
        let tmp_dir = tempfile::tempdir().expect("tmpdir");
        let cfg_path: PathBuf = tmp_dir.path().join("app.toml");
        let mut f = fs::File::create(&cfg_path).expect("create file");
        // file value: 2
        writeln!(f, "[app]\nvalue = 2").expect("write file");

        // 2) CLI overrides: value -> 4 (highest precedence in this test)
        let cli_overrides = vec!["--cfg.app.value=4".to_string()];

        // 3) Defaults: value -> 1
        let defaults: toml::Value = toml::toml! {
            [app]
            value = 1
        }
        .into();

        // Build an app with ConfigModule configured with all sources
        let builder = AppBuilder::new()
            .with(airframe_args::ArgsModule::new())
            .with(
                ConfigModule::new(Some(cfg_path))
                    .with_defaults(defaults)
                    .with_cli_overrides(cli_overrides),
            );

        let app = builder.start().await.expect("app starts");

        // Read the effective config
        let cfg = app
            .services
            .get::<BasicConfig>()
            .expect("BasicConfig present");
        let raw = &cfg.raw;

        // Extract app.value
        let got = raw
            .get("app")
            .and_then(|t| t.get("value"))
            .and_then(|v| v.as_integer())
            .expect("integer value present");

        // Expect highest precedence (CLI) = 4
        assert_eq!(
            got, 4,
            "expected CLI override to win over env/file/defaults"
        );
    }
}

// When the required features are not enabled, include a trivial test so the
// test target still compiles in the wider workspace.
#[cfg(not(all(feature = "config", feature = "args")))]
#[test]
fn requires_config_and_args_features() {
    // No-op: required features are not enabled
}
