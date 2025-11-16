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
        let fd_mappings = open_files
            .map(|(child_fd, open_file)| FdMapping {
                child_fd,
                parent_fd: open_file.into_owned_fd().unwrap(),
            })
            .collect();
        self.fd_mappings(fd_mappings)
            .map_err(|_e| error::ErrorKind::ChildCreationFailure)?;

        Ok(())
    }
}

/// Extension trait for arranging for commands to take the foreground.
pub trait CommandFgControlExt {
    /// Arranges for the command to take the foreground when it is executed.
    fn take_foreground(&mut self);
}

impl CommandFgControlExt for std::process::Command {
    fn take_foreground(&mut self) {
        // SAFETY:
        // This arranges for a provided function to run in the context of
        // the forked process before it exec's the target command. In general,
        // rust can't guarantee safety of code running in such a context.
        unsafe {
            self.pre_exec(setup_process_before_exec);
        }
    }
}

fn setup_process_before_exec() -> Result<(), std::io::Error> {
    use crate::sys;

    sys::terminal::move_self_to_foreground().map_err(std::io::Error::other)?;
    Ok(())
}
