//! Child process management for the host renderer.

use spacetime_ipc::{ChildHandle, IpcError};
use std::path::Path;
use std::process::{Child, Command};

/// A child process handle wrapping `std::process::Child`.
pub struct HostChildProcess {
    child: Child,
}

impl HostChildProcess {
    /// Spawn a child process from the given program path with arguments.
    pub fn spawn<P: AsRef<Path>>(program: P, args: &[&str]) -> Result<Self, IpcError> {
        let child = Command::new(program.as_ref())
            .args(args)
            .spawn()
            .map_err(|_| IpcError::SpawnFailed)?;
        Ok(Self { child })
    }

    /// Get a mutable reference to the underlying `Child`.
    pub fn inner_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

impl ChildHandle for HostChildProcess {
    fn is_alive(&self) -> bool {
        // try_wait is not available on &self, so we check /proc on Linux
        let pid = self.child.id();
        Path::new(&format!("/proc/{pid}")).exists()
    }

    fn pid(&self) -> u64 {
        self.child.id() as u64
    }

    fn kill(&mut self) -> Result<(), IpcError> {
        self.child.kill().map_err(|_| IpcError::KillFailed)?;
        let _ = self.child.wait(); // Reap the child
        Ok(())
    }
}
