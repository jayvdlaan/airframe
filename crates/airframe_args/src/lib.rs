//! Command-line argument capture for Airframe.
//!
//! `airframe_args` collects `std::env::args()`, normalizes it, and registers a
//! [`CliArgs`] value in the `ServiceRegistry`. Other modules (for example
//! `airframe_config` with its `args` feature) require `cap:args` to guarantee
//! ordering and read the captured CLI.
//!
//! # Key pieces
//! - [`CliArgs`] — the normalized, registered CLI snapshot.
//! - [`GlobalFlags`] — common parsed flags.
//! - [`ArgsModule`] — Airframe module that captures argv and provides `cap:args`.
//! - [`cli`] — small argv-parsing helpers shared across modules.
//!
//! # Example
//! ```ignore
//! use airframe_core::app::AppBuilder;
//! use airframe_args::{ArgsModule, CliArgs};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let app = AppBuilder::new().with(ArgsModule::new()).start().await?;
//! let args = app.services.get::<CliArgs>();
//! # Ok(()) }
//! ```
use std::sync::Arc;

use airframe_core::bus::{Event, EventBus};
use airframe_core::module::{Module, ModuleContext, ModuleDescriptor, CAP_ARGS};
use airframe_core::platform::PlatformSupport;
use airframe_macros::module_descriptor;
use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, info};

pub mod cli;

#[derive(Debug, Clone, Default)]
pub struct GlobalFlags {
    pub quiet: bool,
    pub verbose: bool,
    pub json: bool,
    /// Unknown leading flags preserved for forward-compat
    pub passthrough: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CliArgs {
    /// Normalized argument vector used by config and overrides parsing (no program name)
    pub argv: Vec<String>,
    /// Raw process argv including program name (for debugging)
    pub raw: Vec<String>,
    /// Whether a literal "--" separator was present
    pub has_separator: bool,
    /// Parsed global flags (leading flags before first non-flag or before "--")
    pub globals: GlobalFlags,
    /// First positional after globals (or after "--")
    pub command: Option<String>,
    /// Remaining args after command
    pub command_args: Vec<String>,
}

impl CliArgs {
    pub fn new_normalized(raw: Vec<String>) -> Self {
        normalize_argv(raw)
    }
}

fn normalize_argv(raw: Vec<String>) -> CliArgs {
    // raw[0] is program name if present
    let without_prog: Vec<String> = if !raw.is_empty() {
        raw.iter().skip(1).cloned().collect()
    } else {
        vec![]
    };
    let mut has_separator = false;
    let mut globals = GlobalFlags::default();
    let mut command: Option<String> = None;
    let mut command_args: Vec<String> = Vec::new();

    if let Some(pos) = without_prog.iter().position(|a| a == "--") {
        has_separator = true;
        // Globals are before separator
        let globals_slice = &without_prog[..pos];
        parse_global_flags(globals_slice, &mut globals);
        // Command space after separator
        let mut tail = without_prog[pos + 1..].iter().cloned();
        if let Some(cmd) = tail.next() {
            command = Some(cmd);
            command_args = tail.collect();
        }
    } else {
        // Consume leading known global flags until first non-flag token
        let mut i = 0usize;
        while i < without_prog.len() {
            let a = &without_prog[i];
            match a.as_str() {
                "--quiet" => {
                    globals.quiet = true;
                    i += 1;
                }
                "--verbose" => {
                    globals.verbose = true;
                    i += 1;
                }
                "--json" => {
                    globals.json = true;
                    i += 1;
                }
                _ if a.starts_with('-') => {
                    globals.passthrough.push(a.clone());
                    i += 1;
                }
                _ => break,
            }
        }
        if i < without_prog.len() {
            command = Some(without_prog[i].clone());
            command_args = without_prog[i + 1..].to_vec();
        } else {
            command = None;
            command_args = vec![];
        }
    }

    // Build argv for config/overrides: we want to include all flags and args that config layer expects.
    // We reconstruct from without_prog, excluding the separator token itself if present.
    let argv: Vec<String> = if has_separator {
        // Keep everything before and after separator (minus the separator itself)
        let pos = raw
            .iter()
            .skip(1)
            .position(|a| a == "--")
            .unwrap_or(usize::MAX);
        let mut v = Vec::new();
        let wop: Vec<String> = raw.iter().skip(1).cloned().collect();
        if pos != usize::MAX {
            v.extend_from_slice(&wop[..pos]);
            v.extend_from_slice(&wop[pos + 1..]);
            v
        } else {
            wop
        }
    } else {
        without_prog.clone()
    };

    CliArgs {
        argv,
        raw,
        has_separator,
        globals,
        command,
        command_args,
    }
}

fn parse_global_flags(tokens: &[String], out: &mut GlobalFlags) {
    for t in tokens {
        match t.as_str() {
            "--quiet" => out.quiet = true,
            "--verbose" => out.verbose = true,
            "--json" => out.json = true,
            s if s.starts_with('-') => out.passthrough.push(s.to_string()),
            _ => {}
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AppStartup;
impl Event for AppStartup {
    const NAME: &'static str = "AppStartup";
}

/// Simple ArgsModule that normalizes argv and registers CliArgs
pub struct ArgsModule {
    desc: ModuleDescriptor,
}

impl Default for ArgsModule {
    fn default() -> Self {
        Self::new()
    }
}

impl ArgsModule {
    pub fn new() -> Self {
        Self {
            desc: module_descriptor!(
                name: "args",
                version: "0.1.0",
                provides: [CAP_ARGS.0]
            ),
        }
    }
}

#[async_trait]
impl Module for ArgsModule {
    airframe_macros::impl_descriptor!();

    fn platform_support(&self) -> PlatformSupport {
        PlatformSupport::desktop_only("CLI args are not supported on mobile targets")
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        // Collect argv and register normalized args in ServiceRegistry
        let raw: Vec<String> = std::env::args().collect();
        let argc = raw.len();
        let normalized = CliArgs::new_normalized(raw);
        ctx.services.register::<CliArgs>(Arc::new(normalized));
        debug!(target = "airframe_args", argc = argc, "Args collected");
        Ok(())
    }
    async fn start(&mut self) -> Result<()> {
        Ok(())
    }
}

// Legacy helper kept for compatibility with docs/examples in the repo (publishes startup)
pub struct ArgsModuleWithStartup {
    inner: ArgsModule,
}
impl Default for ArgsModuleWithStartup {
    fn default() -> Self {
        Self::new()
    }
}

impl ArgsModuleWithStartup {
    pub fn new() -> Self {
        Self {
            inner: ArgsModule::new(),
        }
    }
}

#[async_trait]
impl Module for ArgsModuleWithStartup {
    fn descriptor(&self) -> &ModuleDescriptor {
        self.inner.descriptor()
    }

    fn platform_support(&self) -> PlatformSupport {
        self.inner.platform_support()
    }

    async fn init(&mut self, ctx: ModuleContext) -> Result<()> {
        let raw: Vec<String> = std::env::args().collect();
        let normalized = CliArgs::new_normalized(raw);
        ctx.services.register::<CliArgs>(Arc::new(normalized));
        info!(target = "airframe_args", "publishing AppStartup");
        if let Some(bus) = ctx
            .services
            .get::<airframe_core::bus::inmem::InMemoryEventBus>()
        {
            let _ = bus.publish(AppStartup, None).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airframe_core::app::AppBuilder;

    #[tokio::test]
    async fn registers_cli_args_normalized() {
        // Build app with ArgsModule and ensure CliArgs present
        let builder = AppBuilder::new().with(ArgsModule::new());
        let app = builder.start().await.expect("app start");
        let args = app.services.get::<CliArgs>().expect("CliArgs registered");
        // argv should be raw without the program name when present
        assert!(args.raw.is_empty() || args.argv.len() == args.raw.len() - 1);
    }

    #[test]
    fn normalization_preserves_unknown_leading_flags_and_separator() {
        // Simulate a raw argv vector with program name followed by globals and separator
        let raw = vec![
            "prog".to_string(),
            "--quiet".to_string(),
            "--x-unknown".to_string(),
            "--json".to_string(),
            "--".to_string(),
            "deploy".to_string(),
            "--not-a-global".to_string(),
            "svc".to_string(),
        ];
        let c = CliArgs::new_normalized(raw);
        assert!(c.has_separator);
        assert!(c.globals.quiet && c.globals.json);
        // Unknown leading flag should be captured in passthrough
        assert_eq!(c.globals.passthrough, vec!["--x-unknown".to_string()]);
        // After separator, first token is treated as command verb
        assert_eq!(c.command.as_deref(), Some("deploy"));
        assert_eq!(
            c.command_args,
            vec!["--not-a-global".to_string(), "svc".to_string()]
        );
        // argv should not include the separator token itself
        assert!(!c.argv.iter().any(|s| s == "--"));
    }

    #[test]
    fn normalization_without_separator_consumes_leading_globals_then_command() {
        let raw = vec![
            "prog".to_string(),
            "--verbose".to_string(),
            "--mystery".to_string(),
            "run".to_string(),
            "--flag".to_string(),
        ];
        let c = CliArgs::new_normalized(raw);
        assert!(!c.has_separator);
        assert!(c.globals.verbose);
        assert_eq!(c.globals.passthrough, vec!["--mystery".to_string()]);
        assert_eq!(c.command.as_deref(), Some("run"));
        assert_eq!(c.command_args, vec!["--flag".to_string()]);
        // argv should equal all tokens after program name
        assert_eq!(
            c.argv,
            ["--verbose", "--mystery", "run", "--flag"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }
}
