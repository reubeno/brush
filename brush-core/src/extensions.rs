//! Definition of shell behavior traits and defaults.

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
}

/// Shell extensions implementation constructed from component types.
#[derive(Clone, Default)]
pub struct ShellExtensionsImpl<
    EF: ErrorFormatter = DefaultErrorFormatter,
    PL: ProcessLauncher = DefaultProcessLauncher,
> {
    _marker: std::marker::PhantomData<(EF, PL)>,
}

impl<EF: ErrorFormatter, PL: ProcessLauncher> ShellExtensions for ShellExtensionsImpl<EF, PL> {
    type ErrorFormatter = EF;
    type ProcessLauncher = PL;
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

/// Trait for placeholder behavior (stub for future extension).
pub trait PlaceholderBehavior: Clone + Default + Send + Sync + 'static {}

/// Default placeholder implementation.
#[derive(Clone, Default)]
pub struct DefaultPlaceholder;

impl PlaceholderBehavior for DefaultPlaceholder {}
