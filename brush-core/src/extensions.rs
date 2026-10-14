//! Definition of shell behavior traits and defaults.

use crate::{Shell, error, extensions, sys};

/// Trait for static shell extensions. Collects all associated types needed to
/// instantiate a shell into a single containing struct.
pub trait ShellExtensions: Clone + Default + Send + Sync + 'static {
    /// Type of the error behavior implementation.
    type ErrorFormatter: ErrorFormatter;
    /// Type of the external command spawner implementation.
    type ExternalCommandSpawner: ExternalCommandSpawner;
}

/// Shell extensions implementation constructed from component types.
#[derive(Clone, Default)]
pub struct ShellExtensionsImpl<
    EF: ErrorFormatter = DefaultErrorFormatter,
    ECS: ExternalCommandSpawner = DefaultExternalCommandSpawner,
> {
    _marker: std::marker::PhantomData<(EF, ECS)>,
}

impl<EF: ErrorFormatter, ECS: ExternalCommandSpawner> ShellExtensions
    for ShellExtensionsImpl<EF, ECS>
{
    type ErrorFormatter = EF;
    type ExternalCommandSpawner = ECS;
}

/// Default shell extensions implementation.
/// This is a type alias for the most common shell configuration.
pub type DefaultShellExtensions = ShellExtensionsImpl<DefaultErrorFormatter>;

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

/// Trait for spawning the processes that run external commands.
///
/// The shell resolves a command name to a builtin, shell function, or external program on
/// its own; only the last of these reaches the spawner. By then the shell has composed a
/// [`std::process::Command`] carrying the resolved executable path, `argv`, environment,
/// working directory, file descriptors, and process-group settings. An implementation may
/// spawn it as-is, or build and spawn a different command in its place (e.g. wrapping the
/// program in a tracer, or running it on another host).
///
/// The returned [`Child`](sys::process::Child) is what the shell waits on and reports through
/// `$?`. An `Err` is mapped to the command's exit status:
/// [`NotFound`](std::io::ErrorKind::NotFound) is reported as command-not-found (127), anything
/// else as failed-to-execute (126).
///
/// An implementation is selected statically as the [`ShellExtensions::ExternalCommandSpawner`]
/// associated type; the instance the shell runs with is supplied via
/// [`CreateOptions::external_command_spawner`](crate::CreateOptions::external_command_spawner)
/// and cloned along with the shell (pipeline stages, subshells, command substitutions).
pub trait ExternalCommandSpawner: Clone + Default + Send + Sync + 'static {
    /// Spawns the given command.
    ///
    /// # Arguments
    ///
    /// * `command` - The fully composed command to spawn.
    /// * `kill_on_drop` - Whether the child should be killed when its handle is dropped; see
    ///   [`CreateOptions::kill_external_commands_on_drop`](crate::CreateOptions::kill_external_commands_on_drop).
    fn spawn(
        &self,
        command: std::process::Command,
        kill_on_drop: bool,
    ) -> std::io::Result<sys::process::Child>;
}

/// Default external command spawner; spawns the command exactly as composed.
#[derive(Clone, Default)]
pub struct DefaultExternalCommandSpawner;

impl ExternalCommandSpawner for DefaultExternalCommandSpawner {
    fn spawn(
        &self,
        command: std::process::Command,
        kill_on_drop: bool,
    ) -> std::io::Result<sys::process::Child> {
        sys::process::spawn(command, kill_on_drop)
    }
}

/// Default shell error behavior implementation.
#[derive(Clone, Default)]
pub struct DefaultErrorFormatter;

impl ErrorFormatter for DefaultErrorFormatter {}

/// Trait for placeholder behavior (stub for future extension).
pub trait PlaceholderBehavior: Clone + Default + Send + Sync + 'static {}

/// Default placeholder implementation.
#[derive(Clone, Default)]
pub struct DefaultPlaceholder;

impl PlaceholderBehavior for DefaultPlaceholder {}
