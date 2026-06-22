// Example showing how to inject config defaults via the contributor registry
// and how to switch prefab profiles.
// Run with:
//   cargo run -p airframe_prefab --features http,config --example defaults_contributor -- --config ./app.toml

#[cfg(all(feature = "http", feature = "config"))]
#[tokio::main]
async fn main() {
    use airframe_prefab::{
        get_or_create_config_defaults_registry, ConfigDefaultsContributor, HttpApiServerPrefab,
        PrefabProfile,
    };
    use std::sync::Arc;

    struct DevCorsDefaults;
    impl ConfigDefaultsContributor for DevCorsDefaults {
        fn defaults(&self) -> toml::Value {
            toml::toml! {
                [cors]
                enable = true
                allow_methods = ["GET", "POST", "OPTIONS"]
                allow_headers = ["Content-Type"]
            }
            .into()
        }
    }

    // Build with a Dev profile: this will merge prefab base defaults with Dev tweaks
    let mut app = HttpApiServerPrefab::new_with_profile(PrefabProfile::Dev)
        .start()
        .await
        .expect("app start");

    // Register a contributor that enables CORS by default (redundant with Dev profile here, for demo)
    let reg = get_or_create_config_defaults_registry(&app.services);
    reg.add(Arc::new(DevCorsDefaults));

    // For example purposes, run briefly and then shutdown
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    app.shutdown().await.expect("shutdown ok");
}

#[cfg(not(all(feature = "http", feature = "config")))]
fn main() {
    eprintln!("This example requires features: http, config");
}
