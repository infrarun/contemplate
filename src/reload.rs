use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::error::Result;
use nix::sys::signal::{kill, Signal, SIGINT};
use nix::unistd::Pid;
use sysinfo::System;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum OnReloadSignalTarget {
    Pid(Pid),
    ProcessName(OsString),
    Parent,
}

impl From<&OsStr> for OnReloadSignalTarget {
    fn from(s: &OsStr) -> Self {
        if s == OsStr::new(":parent") {
            return Self::Parent;
        }

        if let Some(pid) = s.to_str().and_then(|s| s.parse().ok()) {
            return Self::Pid(Pid::from_raw(pid));
        }

        Self::ProcessName(s.to_owned())
    }
}

impl Default for OnReloadSignalTarget {
    fn default() -> Self {
        Self::Parent
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum OnReloadAction {
    /// No action
    None,

    /// Execute a shell command
    ShellCommand(OsString),

    /// Execute an executable
    Executable(OsString),

    /// Signal
    Signal {
        signal: Signal,
        target: OnReloadSignalTarget,
    },
}

pub struct OnReload {
    action: OnReloadAction,
    child: Mutex<Option<Child>>,
}

impl OnReload {
    /// Must be called from the context of a tokio runtime.
    async fn terminate_existing_child(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        if let Some(mut child) = child.take() {
            if let Some(pid) = child.id() {
                kill(Pid::from_raw(pid as _), SIGINT)?;
                tokio::spawn(async move { child.wait().await });
            }
        }

        Ok(())
    }

    /// Must be called from the context of a tokio runtime.
    pub async fn execute<'a, F, P>(&self, updated_files: F) -> Result<()>
    where
        F: Iterator<Item = P>,
        P: Into<Cow<'a, Path>>,
    {
        let contemplated_files: OsString = updated_files
            .map(|path| path.into().as_os_str().to_owned())
            .intersperse(OsString::from(","))
            .collect();

        match self.action {
            OnReloadAction::None => {}
            OnReloadAction::ShellCommand(ref cmd) => {
                let mut command = Command::new("/bin/sh");
                command
                    .arg("-c")
                    .arg(cmd)
                    .env("CONTEMPLATED_FILES", contemplated_files);
                self.terminate_existing_child().await?;
                let child = command.spawn()?;
                *self.child.lock().await = Some(child);
            }
            OnReloadAction::Executable(ref executable) => {
                let mut command = Command::new(executable);
                command.env("CONTEMPLATED_FILES", contemplated_files);
                self.terminate_existing_child().await?;
                let child = command.spawn()?;
                *self.child.lock().await = Some(child);
            }
            OnReloadAction::Signal {
                ref signal,
                ref target,
            } => {
                match target {
                    OnReloadSignalTarget::Pid(pid) => {
                        log::debug!("Sending signal {signal} to PID {pid}");
                        kill(*pid, *signal)?;
                    }
                    OnReloadSignalTarget::Parent => {
                        let pid = Pid::parent();
                        log::debug!("Sending signal {signal} to parent PID {pid}");
                        kill(pid, *signal)?;
                    }
                    OnReloadSignalTarget::ProcessName(name) => {
                        let sys = System::new_all();

                        let processes = name
                            .to_str()
                            .map(|name| sys.processes_by_name(name))
                            .map(|iter| -> Box<dyn Iterator<Item = &sysinfo::Process>> {
                                Box::new(iter)
                            })
                            .unwrap_or(Box::new(std::iter::empty()));

                        for process in processes {
                            log::debug!(
                                "Sending signal {signal} to {} (PID {})",
                                process.name(),
                                process.pid()
                            );
                            let pid = Pid::from_raw(process.pid().as_u32() as _);
                            kill(pid, *signal)?;
                        }
                    }
                };
            }
        }

        Ok(())
    }
}

impl From<OnReloadAction> for OnReload {
    fn from(action: OnReloadAction) -> Self {
        Self {
            action,
            child: Mutex::new(None),
        }
    }
}
