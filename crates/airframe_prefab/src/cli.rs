//! Minimal CLI dispatching helpers for Airframe prefabs.

use airframe_core::registry::ServiceRegistry;
use std::sync::Arc;

/// Trait for a CLI command implementation.
pub trait CliCommand: Send + Sync {
    /// The command name used on the command line (e.g., "hello").
    fn name(&self) -> &'static str;
    /// Execute the command. Return process exit code (0 for success).
    fn run<'a>(
        &'a self,
        services: &'a ServiceRegistry,
        args: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send + 'a>>;
    /// Short help shown in the dispatcher help text.
    fn help(&self) -> &'static str {
        ""
    }
}

/// Simple in-memory registry for CLI commands.
#[derive(Default)]
pub struct CliRegistry {
    cmds: Vec<Arc<dyn CliCommand>>,
}
impl CliRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register<C: CliCommand + 'static>(mut self, cmd: C) -> Self {
        self.cmds.push(Arc::new(cmd));
        self
    }
    pub fn with_command<C: CliCommand + 'static>(mut self, cmd: C) -> Self {
        self.cmds.push(Arc::new(cmd));
        self
    }
    pub fn add<C: CliCommand + 'static>(&mut self, cmd: C) {
        self.cmds.push(Arc::new(cmd));
    }

    /// Dispatch based on the first arg. If none or unknown, prints help and returns 2.
    pub async fn dispatch(&self, services: &ServiceRegistry, args: &[String]) -> i32 {
        let mut iter = args.iter();
        let cmd_name = iter.next().map(|s| s.as_str());
        match cmd_name {
            None => {
                Self::print_help(&self.cmds);
                2
            }
            Some(name) => {
                if let Some(cmd) = self.cmds.iter().find(|c| c.name() == name) {
                    let rest: Vec<String> = iter.cloned().collect();
                    cmd.run(services, &rest).await
                } else {
                    eprintln!("Unknown command: {name}\n");
                    Self::print_help(&self.cmds);
                    2
                }
            }
        }
    }

    fn print_help(cmds: &Vec<Arc<dyn CliCommand>>) {
        eprintln!("Usage: <binary> <command> [args...]\n");
        if cmds.is_empty() {
            eprintln!("No commands registered.");
            return;
        }
        eprintln!("Available commands:");
        for c in cmds {
            let h = c.help();
            if h.is_empty() {
                eprintln!("  {}", c.name());
            } else {
                eprintln!("  {:16} {}", c.name(), h);
            }
        }
    }
}
