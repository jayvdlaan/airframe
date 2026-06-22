// Minimal CLI prefab example with subcommand dispatch
// Run:
//   cargo run -p airframe_prefab --example cli -- hello World
//   cargo run -p airframe_prefab --example cli -- config print
// Pass a config file with:
//   cargo run -p airframe_prefab --example cli -- --config ./config.toml hello

#![forbid(unsafe_code)]

use airframe_core::registry::ServiceRegistry;
use airframe_prefab::cli::{CliCommand, CliRegistry};
use airframe_prefab::CliPrefab;
use tracing::{info, trace};

struct HelloCmd;
impl CliCommand for HelloCmd {
    fn name(&self) -> &'static str {
        "hello"
    }
    fn help(&self) -> &'static str {
        "Prints a friendly greeting"
    }
    fn run<'a>(
        &'a self,
        _services: &'a ServiceRegistry,
        args: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send + 'a>> {
        Box::pin(async move {
            let who = args.first().map(|s| s.as_str()).unwrap_or("world");
            trace!(target = "example.cli", who = who, "hello command starting");
            println!("Hello, {who}!");
            info!(target = "example.cli", who = who, "said hello");
            trace!(target = "example.cli", "hello command finished");
            0
        })
    }
}

struct ConfigPrintCmd;
impl CliCommand for ConfigPrintCmd {
    fn name(&self) -> &'static str {
        "config"
    }
    fn help(&self) -> &'static str {
        "Config subcommands (try: 'config print')"
    }
    fn run<'a>(
        &'a self,
        _services: &'a ServiceRegistry,
        args: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send + 'a>> {
        Box::pin(async move {
            match args.first().map(|s| s.as_str()) {
                Some("print") => {
                    trace!(target = "example.cli", "config print invoked");
                    // In a real app, fetch the effective config from services and pretty-print it.
                    println!("(demo) Effective config would be printed here");
                    trace!(target = "example.cli", "config print finished");
                    0
                }
                _ => {
                    eprintln!("Usage: config print");
                    2
                }
            }
        })
    }
}

#[tokio::main]
async fn main() {
    // Parse global CLI flags before module initialization so logging picks them up.
    // Flags before "--" are treated as global: --quiet | --verbose | --json
    let mut level: Option<&'static str> = None;
    let mut json: bool = false;
    for a in std::env::args().skip(1) {
        if a == "--" {
            break;
        }
        match a.as_str() {
            "--quiet" => level = Some("warn"),
            "--verbose" => level = Some("debug"),
            "--json" => json = true,
            _ => {}
        }
    }
    // Note: In this example we only parse flags. Wiring them into airframe_logging
    // is done via config/env in real apps. This example avoids mutating env vars.
    if let Some(lvl) = level {
        eprintln!("[demo] would set log level to {lvl}");
    }
    if json {
        eprintln!("[demo] would enable JSON log format");
    }

    // Start from the CLI prefab and start the app (modules init/start lifecycle)
    // Opt-in to early minimal bootstrap logger so very-early logs are captured to stderr.
    // This remains a best-effort install and will not interfere with a later full logger.
    let builder = CliPrefab::new();
    match builder.start().await {
        Ok(mut app) => {
            // Use normalized CliArgs from airframe_args when the 'args' feature is enabled.
            // Otherwise, just run the demo command registry with no args.
            #[cfg(feature = "args")]
            let dispatch_vec: Vec<String> = {
                let args = app
                    .services
                    .get::<airframe_args::CliArgs>()
                    .expect("CliArgs present");
                if args.globals.verbose {
                    eprintln!("[demo] verbose enabled");
                }
                if args.globals.quiet {
                    eprintln!("[demo] quiet enabled");
                }
                if args.globals.json {
                    eprintln!("[demo] json enabled");
                }
                match &args.command {
                    Some(cmd) => {
                        let mut v = Vec::with_capacity(1 + args.command_args.len());
                        v.push(cmd.clone());
                        v.extend(args.command_args.clone());
                        v
                    }
                    None => vec![],
                }
            };
            #[cfg(not(feature = "args"))]
            let dispatch_vec: Vec<String> = {
                eprintln!("[demo] 'args' feature is not enabled; running with no CLI command");
                vec![]
            };

            // Build a small command registry and dispatch based on normalized command and args
            let reg = CliRegistry::new()
                .register(HelloCmd)
                .register(ConfigPrintCmd);
            let code = reg.dispatch(&app.services, &dispatch_vec).await;
            // Shutdown and exit with the command's code
            let _ = app.shutdown().await;
            if code != 0 {
                std::process::exit(code);
            }
        }
        Err(e) => {
            eprintln!("CLI failed to start: {e}");
            std::process::exit(1);
        }
    }
}
