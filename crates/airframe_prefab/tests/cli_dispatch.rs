#![forbid(unsafe_code)]

use airframe_core::registry::ServiceRegistry;
use airframe_prefab::cli::{CliCommand, CliRegistry};

struct OkCmd;
impl CliCommand for OkCmd {
    fn name(&self) -> &'static str {
        "ok"
    }
    fn help(&self) -> &'static str {
        "returns 0"
    }
    fn run<'a>(
        &'a self,
        _services: &'a ServiceRegistry,
        _args: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send + 'a>> {
        Box::pin(async move { 0 })
    }
}

struct EchoCmd;
impl CliCommand for EchoCmd {
    fn name(&self) -> &'static str {
        "echo"
    }
    fn help(&self) -> &'static str {
        "echoes one arg; returns 0"
    }
    fn run<'a>(
        &'a self,
        _services: &'a ServiceRegistry,
        args: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send + 'a>> {
        Box::pin(async move {
            if let Some(s) = args.first() {
                println!("{}", s);
                0
            } else {
                2
            }
        })
    }
}

#[tokio::test]
async fn dispatch_known_command_returns_zero() {
    let services = ServiceRegistry::default();
    let reg = CliRegistry::new().register(OkCmd).register(EchoCmd);
    let code = reg.dispatch(&services, &["ok".to_string()]).await;
    assert_eq!(code, 0);
}

#[tokio::test]
async fn dispatch_unknown_command_returns_two() {
    let services = ServiceRegistry::default();
    let reg = CliRegistry::new().register(OkCmd);
    let code = reg
        .dispatch(&services, &["does-not-exist".to_string()])
        .await;
    assert_eq!(code, 2);
}

#[tokio::test]
async fn dispatch_no_args_returns_two_and_prints_help() {
    let services = ServiceRegistry::default();
    let reg = CliRegistry::new().register(OkCmd);
    let code = reg.dispatch(&services, &[]).await;
    assert_eq!(code, 2);
}
