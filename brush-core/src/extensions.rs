//! The behaviors a shell is built with, as types its host names.
//!
//! [`ShellExtensions`] bundles them. They come in two families:
//!
//! * What the host provides. The process a shell runs in belongs to whoever hosts it, which
//!   may run several shells, or have uses of its own for the process; so what the shell must
//!   keep asking of that process is a contract the host implements: [`ProcessLauncher`]
//!   starts the programs the shell runs, and [`Terminal`] moves the terminal's foreground
//!   for job control. Each has a stock implementation that does nothing, for a host that
//!   hasn't got the thing, or won't share it: [`NoProcessLauncher`] and [`NoTerminal`]. A
//!   stock implementation that changes nothing process-wide lives here and is the default
//!   ([`DefaultProcessLauncher`] starts native processes); one that takes over something
//!   the whole process shares, such as its terminal, lives in the `brush-hosting` crate, and
//!   the default here is the one that does nothing.
//! * How the shell speaks. [`ErrorFormatter`] renders errors. Nothing about owning the
//!   process changes it; it's presentation, open to the host all the same.

use crate::{
    Shell, error, extensions,
    processes::{ChildProcess, LaunchRequest},
    sys,
};

/// Trait for static shell extensions. Collects all associated types needed to
/// instantiate a shell into a single containing struct.
pub trait ShellExtensions: Clone + Default + Send + Sync + 'static {
    /// Type of the error behavior implementation.
    type ErrorFormatter: ErrorFormatter;
    /// Type of the process launcher implementation.
    type ProcessLauncher: ProcessLauncher;
    /// Type of the terminal the shell's job control uses.
    type Terminal: Terminal;
}

/// Shell extensions implementation constructed from component types.
#[derive(Clone, Default)]
pub struct ShellExtensionsImpl<
    EF: ErrorFormatter = DefaultErrorFormatter,
    PL: ProcessLauncher = DefaultProcessLauncher,
    T: Terminal = NoTerminal,
> {
    _marker: std::marker::PhantomData<(EF, PL, T)>,
}

impl<EF: ErrorFormatter, PL: ProcessLauncher, T: Terminal> ShellExtensions
    for ShellExtensionsImpl<EF, PL, T>
{
    type ErrorFormatter = EF;
    type ProcessLauncher = PL;
    type Terminal = T;
}

/// Default shell extensions implementation.
/// This is a type alias for the most common shell configuration.
pub type DefaultShellExtensions = ShellExtensionsImpl;

/// Trait for defining shell error behaviors.
pub trait ErrorFormatter: Clone + Default + Send + Sync + 'static {
    /// Format the given error for display within the context of the provided shell.
    ///
    /// # Arguments
    ///
    /// * `error` - The error to format
    /// * `shell` - The shell context in which the error occurred.
    fn format_error(
        &self,
        error: &error::Error,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> String {
        let _ = shell;
        std::format!("error: {error:#}\n")
    }
}

/// Default shell error behavior implementation.
#[derive(Clone, Default)]
pub struct DefaultErrorFormatter;

impl ErrorFormatter for DefaultErrorFormatter {}

/// Trait for launching the processes a shell runs.
///
/// These are the external programs that commands resolve to, once the shell has
/// set up everything they start with. A host provides its own launcher to launch
/// them some other way (e.g., in a sandbox, or where the platform can't start
/// processes).
///
/// # Limitations
///
/// Only processes the [`DefaultProcessLauncher`] starts (including for a host's
/// launcher that hands a request on to it) are native children of the shell.
/// Those a host's launcher makes itself, with [`ChildProcess::from_future`],
/// have no process ID, and so take no part in job control:
///
/// * `$!` expands to nothing for them, and `jobs -p` has no ID to list.
/// * `kill` can't signal them, and they can't be stopped (e.g., with Ctrl-Z) or
///   continued. The shell can only drop them, if [`LaunchRequest::kill_on_drop`]
///   asks it to.
/// * The shell doesn't forward signals to them: Ctrl-C reaches one only if it's
///   in the terminal's foreground process group. Waiting for one ends only when
///   its future does.
/// * They have no process group for others to join: under job control, each
///   pipeline stage after the first is asked to run in the shell's group
///   ([`ProcessGroup::Inherit`](crate::processes::ProcessGroup::Inherit)), not
///   the first stage's.
pub trait ProcessLauncher: Clone + Default + Send + Sync + 'static {
    /// Starts a process.
    ///
    /// The error starting it decides what the shell reports: one of kind
    /// [`std::io::ErrorKind::NotFound`] means the command could not be found
    /// (status 127); any other, that it could not be executed (status 126).
    ///
    /// Once started, the process's result is that of the future it was made
    /// from. If that future fails, the shell reports the error as it would any
    /// error running a command, and the command's status is 1.
    ///
    /// # Arguments
    ///
    /// * `request` - The process to start.
    fn spawn(&self, request: LaunchRequest) -> Result<ChildProcess, std::io::Error>;

    /// Replaces the shell's process with a new one, for `exec`. Returns only if
    /// that fails, with the reason why.
    ///
    /// By default, it fails as unsupported.
    ///
    /// # Arguments
    ///
    /// * `request` - The process to replace the shell's with.
    fn exec(&self, request: LaunchRequest) -> std::io::Error {
        let _ = request;
        std::io::ErrorKind::Unsupported.into()
    }
}

/// Default process launcher implementation: launches native processes, where
/// the platform can start them. Elsewhere (e.g., on wasm), every launch fails as
/// unsupported.
#[derive(Clone, Default)]
pub struct DefaultProcessLauncher;

impl ProcessLauncher for DefaultProcessLauncher {
    fn spawn(&self, request: LaunchRequest) -> Result<ChildProcess, std::io::Error> {
        sys::process::spawn(request)
    }

    fn exec(&self, request: LaunchRequest) -> std::io::Error {
        sys::process::exec(request)
    }
}

/// A launcher that starts nothing, for a host that wants a shell of builtins and functions alone.
///
/// Every launch fails as unsupported, so that a command that resolves to a program reports
/// that it could not be executed (status 126).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoProcessLauncher;

impl ProcessLauncher for NoProcessLauncher {
    fn spawn(&self, _request: LaunchRequest) -> Result<ChildProcess, std::io::Error> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

/// Trait for the terminal a shell's job control uses.
///
/// Under job control, the shell gives the terminal's foreground to each job it runs in the
/// foreground, and takes it back when the job ends or stops. Which terminal that is, and
/// whether there is one at all, is the host's: a program that owns its process and has a
/// controlling terminal provides one (as the `brush-hosting` crate does); an embedder without
/// one, or that shares its terminal with something else, provides [`NoTerminal`].
pub trait Terminal: Clone + Default + Send + Sync + 'static {
    /// Gives the terminal's foreground to a process group, for the job it runs.
    ///
    /// # Arguments
    ///
    /// * `group` - The process group to give the foreground to.
    fn give_foreground(&self, group: sys::process::ProcessId) -> Result<(), std::io::Error>;

    /// Takes the terminal's foreground back, for the shell's own process group.
    fn take_foreground(&self) -> Result<(), std::io::Error>;
}

/// No terminal: the foreground is never moved. The default, for a shell whose host hasn't
/// given it a terminal.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTerminal;

impl Terminal for NoTerminal {
    fn give_foreground(&self, _group: sys::process::ProcessId) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn take_foreground(&self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

/// Trait for placeholder behavior (stub for future extension).
pub trait PlaceholderBehavior: Clone + Default + Send + Sync + 'static {}

/// Default placeholder implementation.
#[derive(Clone, Default)]
pub struct DefaultPlaceholder;

impl PlaceholderBehavior for DefaultPlaceholder {}
