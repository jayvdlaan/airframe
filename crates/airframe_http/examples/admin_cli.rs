#![allow(unexpected_cfgs)]

#[cfg(feature = "client")]
use std::sync::Arc;
#[cfg(feature = "client")]
use std::time::Duration;

#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use anyhow::Result;
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use async_trait::async_trait;

#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use airframe_core::app::AppBuilder;
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use airframe_core::module::{
    Module, ModuleContext, ModuleDescriptor, CAP_CLI_ADMIN, CAP_HTTP_CLIENT, CAP_HTTP_SERVER,
    CAP_LOGGING,
};
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use semver::Version;
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use tracing::info;

#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use airframe_http::admin::AdminModule;
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use airframe_http::axum_server::{AxumServerModule, BoundAddr};
#[cfg(all(
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
use airframe_http::clients::client_module::ReqwestClientModule;
#[cfg(all(
    feature = "server",
    feature = "client",
    airframe_http_admin_cli_example
))]
use airframe_http::{HttpClient, SpecClient};

#[cfg(all(
    feature = "server",
    feature = "client",
    airframe_http_admin_cli_example
))]
use airframe_api::http::Uri;
#[cfg(all(
    feature = "server",
    feature = "client",
    airframe_http_admin_cli_example
))]
use airframe_api::CodeSpec;

// NOTE: This example depends on the separate airframe_cli crate which isn't wired into typical
// test builds for this workspace. To avoid breaking test builds, we gate these imports behind a
// never-used cfg flag. Enable `--cfg airframe_http_admin_cli_example` explicitly to build it.
#[cfg(all(
    feature = "server",
    feature = "client",
    airframe_http_admin_cli_example
))]
use airframe_cli::demo::ClockModule;
#[cfg(all(
    feature = "server",
    feature = "client",
    airframe_http_admin_cli_example
))]
use airframe_cli::runtime::{
    get_or_create_command_registry, get_or_create_value_registry, get_or_create_widget_registry,
    CliRuntimeModule,
};
#[cfg(all(
    feature = "server",
    feature = "client",
    airframe_http_admin_cli_example
))]
use airframe_cli::State;

#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "config",
    airframe_http_admin_cli_example
))]
use airframe_config::ConfigModule;

// A minimal Admin CLI module that polls /admin/health and exposes admin.refresh
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
struct AdminCliModule {
    desc: ModuleDescriptor,
    ctx: Option<ModuleContext>,
}
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
impl AdminCliModule {
    fn new() -> Self {
        Self {
            desc: ModuleDescriptor {
                name: "admin-cli",
                version: Version::parse("0.1.0").unwrap(),
                provides: &[CAP_CLI_ADMIN.0],
                requires: &[CAP_HTTP_SERVER.0, CAP_HTTP_CLIENT.0],
                optional_requires: &[CAP_LOGGING.0],
                requires_with_versions: &[],
                optional_requires_with_versions: &[],
            },
            ctx: None,
        }
    }
}

// Wrapper to allow using Arc<dyn HttpClient> with SpecClient generics
#[cfg(all(feature = "client", airframe_http_admin_cli_example))]
struct ClientArc(Arc<dyn HttpClient<Error = reqwest::Error>>);
#[cfg(all(feature = "client", airframe_http_admin_cli_example))]
impl HttpClient for ClientArc {
    type Error = reqwest::Error;
    fn call(
        &self,
        req: http::Request<airframe_http::bytes::Bytes>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<http::Response<airframe_http::bytes::Bytes>, Self::Error>,
                > + Send,
        >,
    > {
        self.0.call(req)
    }
}

#[cfg(all(feature = "client", airframe_http_admin_cli_example))]
struct AdminHealthProvider {
    client: Arc<airframe_http::SpecClient<ClientArc, CodeSpec>>,
}

#[cfg(all(feature = "client", airframe_http_admin_cli_example))]
impl airframe_cli::ValueProvider for AdminHealthProvider {
    fn bindings(&self) -> Vec<airframe_cli::ValueBinding> {
        let fetch: airframe_cli::ValueFetchFn = {
            let client = self.client.clone();
            Arc::new(move |_state: State| {
                let client = client.clone();
                Box::pin(async move {
                    let resp = client.invoke("health", &serde_json::json!({}), None).await;
                    match resp {
                        Ok(r) => {
                            let body = r.into_body();
                            let val: serde_json::Value = serde_json::from_slice(&body).unwrap_or(
                                serde_json::json!({"raw": String::from_utf8_lossy(&body)}),
                            );
                            Ok::<serde_json::Value, anyhow::Error>(val)
                        }
                        Err(e) => Ok::<serde_json::Value, anyhow::Error>(
                            serde_json::json!({"error": e.to_string()}),
                        ),
                    }
                })
            })
        };
        vec![airframe_cli::ValueBinding {
            path: "admin.health",
            refresh: airframe_cli::RefreshPolicy::Interval(Duration::from_secs(2)),
            fetch,
        }]
    }
}

#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
#[async_trait]
impl Module for AdminCliModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        info!(target = "airframe_http", "AdminCliModule init");
        self.ctx = Some(ctx);
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        let ctx = self.ctx.as_ref().expect("init was not called").clone();
        let svcs = &ctx.services;

        // Discover bound address published by AxumServerModule
        let bound = svcs
            .get::<BoundAddr>()
            .expect("BoundAddr should be published by AxumServerModule");
        let base = format!("http://{}", (bound.0));
        let uri: Uri = base.parse().expect("valid base URI");

        // Build CodeSpec for Admin API and construct a SpecClient using shared HttpClient
        let spec = AdminModule::codespec(uri);
        #[cfg(all(feature = "client", airframe_http_admin_cli_example))]
        let http_client = svcs
            .get::<dyn HttpClient<Error = reqwest::Error>>()
            .expect("reqwest HttpClient in registry");
        #[cfg(all(feature = "client", airframe_http_admin_cli_example))]
        let client = Arc::new(SpecClient::new(ClientArc(http_client), spec));

        // Register ValueProvider for admin.health
        #[cfg(all(feature = "client", airframe_http_admin_cli_example))]
        {
            let vreg = get_or_create_value_registry(svcs);
            vreg.add(Arc::new(AdminHealthProvider {
                client: client.clone(),
            }));
        }

        // Register admin.refresh command to force-refresh the value
        #[cfg(all(feature = "client", airframe_http_admin_cli_example))]
        {
            let creg = get_or_create_command_registry(svcs);
            let handler: airframe_cli::CommandHandlerFn = {
                let client = client.clone();
                Arc::new(move |_args: airframe_cli::CommandArgs, state: State| {
                    let client = client.clone();
                    Box::pin(async move {
                        let resp = client.invoke("health", &serde_json::json!({}), None).await;
                        match resp {
                            Ok(r) => {
                                let body = r.into_body();
                                let val: serde_json::Value = serde_json::from_slice(&body)
                                    .unwrap_or(
                                        serde_json::json!({"raw": String::from_utf8_lossy(&body)}),
                                    );
                                state.set("admin.health", val);
                            }
                            Err(e) => {
                                state.set(
                                    "admin.health",
                                    serde_json::json!({"error": e.to_string()}),
                                );
                            }
                        }
                        Ok(())
                    })
                })
            };
            creg.add_all(vec![airframe_cli::CommandSpec {
                name: "admin.refresh",
                help: "Refresh admin health",
                params: vec![],
                handler,
            }]);
        }

        // Ensure the text widget factory is registered (ClockModule also does this)
        #[cfg(airframe_http_admin_cli_example)]
        let _w = get_or_create_widget_registry(svcs);

        Ok(())
    }
}

// Example main that composes HTTP server + admin routes + reqwest client + CLI runtime
#[cfg(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
))]
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Build a layout that shows admin.health using the simple text widget
    #[cfg(feature = "config")]
    let cfg_defaults: toml::Value = toml::toml! {
        [http]
        bind_addr = "127.0.0.1:8080"
        graceful_ms = 500
        [cli]
        refresh_ms = 250
        layout = { Widget = { id = "admin", kind = "text", binding = "admin.health", props = {} } }
    }
    .into();

    // Compose modules
    let mut builder = AppBuilder::new();

    #[cfg(feature = "config")]
    {
        builder = builder.with(ConfigModule::new(None).with_defaults(cfg_defaults));
    }

    // HTTP server with admin routes
    builder = builder
        .with(AxumServerModule::new("127.0.0.1:8080".parse().unwrap()))
        .with(AdminModule::new("airframe", "0.1.0"));

    // HTTP client (reqwest)
    #[cfg(feature = "client")]
    {
        builder = builder.with(ReqwestClientModule::new());
    }

    // CLI runtime + demo clock widget (provides the text widget factory)
    builder = builder
        .with(ClockModule::new())
        .with(AdminCliModule::new())
        .with(CliRuntimeModule::new());

    // Start the app
    let app = builder.start().await?;

    // The app will run until cancelled; pressing 'q' in the CLI cancels it
    app.run_until_cancelled().await
}

#[cfg(not(all(
    feature = "server",
    feature = "client",
    feature = "module",
    airframe_http_admin_cli_example
)))]
fn main() {
    eprintln!("This example is disabled by default. Enable with: RUSTFLAGS='--cfg airframe_http_admin_cli_example' cargo run -p airframe_http --example admin_cli --features server,client,module,config");
}
