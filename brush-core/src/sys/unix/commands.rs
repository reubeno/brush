//! Command execution utilities.

pub use std::os::unix::process::CommandExt;
pub use std::os::unix::process::ExitStatusExt;

use command_fds::{CommandFdExt, FdMapping};

use crate::ShellFd;
use crate::error;
use crate::openfiles;

/// Extension trait for injecting file descriptors into commands.
pub trait CommandFdInjectionExt {
    /// Injects the given open files as file descriptors into the command.
    ///
    /// # Arguments
    ///
    /// * `open_files` - A mapping of child file descriptors to open files.
    fn inject_fds(
        &mut self,
        open_files: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) -> Result<(), error::Error>;
}

impl CommandFdInjectionExt for std::process::Command {
    fn inject_fds(
        &mut self,
        open_files: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) -> Result<(), error::Error> {
        let fd_mappings: Vec<FdMapping> = open_files
            .map(|(child_fd, open_file)| -> Result<FdMapping, error::Error> {
                let parent_fd = open_file.try_clone_to_owned()?;
                Ok(FdMapping {
                    child_fd,
                    parent_fd,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.fd_mappings(fd_mappings)
            .map_err(|_e| error::ErrorKind::ChildCreationFailure)?;

        Ok(())
    }
}

/// Extension trait for arranging for commands to take the foreground.
pub trait CommandFgControlExt {
    /// Arranges for the command to take the foreground when it is executed.
    fn take_foreground(&mut self);

    /// Arranges for the command to become a session leader when it is executed.
    fn lead_session(&mut self);

    /// Arranges for child job-control signals to use their default dispositions.
    fn reset_job_control_signals(&mut self);
}

impl CommandFgControlExt for std::process::Command {
    fn take_foreground(&mut self) {
        // SAFETY:
        // This arranges for a provided function to run in the context of
        // the forked process before it exec's the target command. In general,
        // rust can't guarantee safety of code running in such a context.
        unsafe {
            self.pre_exec(pre_exec_take_foreground);
        }
    }

    fn lead_session(&mut self) {
        // SAFETY:
        // This arranges for a provided function to run in the context of
        // the forked process before it exec's the target command. In general,
        // rust can't guarantee safety of code running in such a context.
        unsafe {
            self.pre_exec(pre_exec_lead_session);
        }
    }

    fn reset_job_control_signals(&mut self) {
        // SAFETY:
        // This arranges for a provided function to run in the context of
        // the forked process before it exec's the target command.
        unsafe {
            self.pre_exec(reset_job_control_signals);
        }
    }
}

fn pre_exec_take_foreground() -> Result<(), std::io::Error> {
    use crate::sys;

    sys::terminal::move_self_to_foreground()?;
    reset_job_control_signals()?;
    Ok(())
}

fn pre_exec_lead_session() -> Result<(), std::io::Error> {
    if let Err(e) = nix::unistd::setsid() {
        return Err(std::io::Error::other(format!(
            "failed to become session leader: {e}"
        )));
    }

    #[cfg(not(target_os = "macos"))]
    let control = libc::TIOCSCTTY;
    #[cfg(target_os = "macos")]
    let control: u64 = libc::TIOCSCTTY.into();

    // SAFETY:
    // This is calling a libc function to set the controlling terminal.
    let result = unsafe { libc::ioctl(0, control, 0) };
    if result != 0 {
        return Err(std::io::Error::other("failed to set controlling terminal"));
    }

    reset_job_control_signals()?;
    Ok(())
}

/// Reset job control signals to `SIG_DFL` (default disposition).
/// This undoes any `SIG_IGN` settings inherited from parent.
fn reset_job_control_signals() -> Result<(), std::io::Error> {
    use nix::sys::signal::{SigHandler, SigSet, SigmaskHow, Signal, signal, sigprocmask};

    // These signals should have default disposition in child processes even if
    // the parent shell ignores them. SIGPIPE is critical for pipeline handling:
    // without it, writing to closed pipes can block indefinitely.
    let signals = [
        Signal::SIGINT,
        Signal::SIGQUIT,
        Signal::SIGTSTP,
        Signal::SIGTTOU,
        Signal::SIGTTIN,
        Signal::SIGPIPE,
    ];

    for sig in signals {
        // SAFETY:
        // This runs in the forked child before exec. Resetting inherited signal
        // dispositions with signal(2) is async-signal-safe.
        unsafe {
            signal(sig, SigHandler::SigDfl).map_err(std::io::Error::from)?;
        }
    }

    let mut sigset = SigSet::empty();
    for sig in signals {
        sigset.add(sig);
    }
    sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&sigset), None).map_err(std::io::Error::from)?;

    Ok(())
}
