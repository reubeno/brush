//! Command execution utilities.

use std::ffi::OsStr;

use crate::{ShellFd, error, openfiles};

/// Extension trait for Unix-like command extensions.
pub trait CommandExt {
    /// Sets the zeroth argument (argv[0]) of the command.
    ///
    /// # Arguments
    ///
    /// * `arg` - The argument to set as argv[0].
    fn arg0<S>(&mut self, arg: S) -> &mut Self
    where
        S: AsRef<OsStr>;

    /// Sets the process group ID of the command.
    ///
    /// # Arguments
    ///
    /// * `pgroup` - The process group ID to set.
    fn process_group(&mut self, pgroup: i32) -> &mut Self;
}

impl CommandExt for std::process::Command {
    fn arg0<S>(&mut self, _arg: S) -> &mut Self
    where
        S: AsRef<OsStr>,
    {
        // NOTE: no-op.
        self
    }

    fn process_group(&mut self, _pgroup: i32) -> &mut Self {
        // NOTE: no-op.
        self
    }
}

/// Extension trait for Unix-like exit status extensions.
pub trait ExitStatusExt {
    /// Returns the signal that terminated the process, if any.
    fn signal(&self) -> Option<i32>;
}

impl ExitStatusExt for std::process::ExitStatus {
    fn signal(&self) -> Option<i32> {
        None
    }
}

/// Replaces the current process with `command`; not supported on this platform.
pub(crate) fn exec(_command: std::process::Command) -> std::io::Error {
    std::io::ErrorKind::Unsupported.into()
}

/// Returns the standard stream for a process to have `file` as.
pub(crate) fn stdio_for(file: openfiles::OpenFile) -> std::io::Result<std::process::Stdio> {
    use std::process::Stdio;

    match file {
        // The shell's own output streams go to whichever stream the process has them as;
        // e.g., the shell's stderr as the process's stdout, for `>&2`.
        // TODO(redirection): the shell's stdin as an output stream (e.g., `>&0`) is
        // inherited from the stream it replaces instead.
        openfiles::OpenFile::Stdin(_) => Ok(Stdio::inherit()),
        openfiles::OpenFile::Stdout(s) => Ok(s.into()),
        openfiles::OpenFile::Stderr(s) => Ok(s.into()),
        openfiles::OpenFile::File(f) => Ok(f.try_clone()?.into()),
        openfiles::OpenFile::PipeReader(r) => Ok(r.try_clone()?.into()),
        openfiles::OpenFile::PipeWriter(w) => Ok(w.try_clone()?.into()),
        // A custom stream has no handle to give a process.
        openfiles::OpenFile::Stream(_) => Ok(Stdio::null()),
    }
}

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
        mut open_files: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) -> Result<(), error::Error> {
        if open_files.next().is_some() {
            return Err(error::ErrorKind::NotSupportedOnThisPlatform("fd redirections").into());
        }

        Ok(())
    }
}

/// Extension trait for arranging for commands to take the foreground.
pub trait CommandFgControlExt {
    /// Arranges for the command to take the foreground when it is executed.
    fn take_foreground(&mut self);
    /// Arranges for the command to become a session leader when it is executed.
    fn lead_session(&mut self);
}

impl CommandFgControlExt for std::process::Command {
    fn take_foreground(&mut self) {
        // NOTE: This is a no-op.
    }

    fn lead_session(&mut self) {
        // NOTE: This is a no-op.
    }
}
