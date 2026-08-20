//! Definition of shell behavior traits and defaults.

use std::path::Path;

use crate::{Shell, error, extensions};

/// Trait for static shell extensions. Collects all associated types needed to
/// instantiate a shell into a single containing struct.
pub trait ShellExtensions: Clone + Default + Send + Sync + 'static {
    /// Type of the error behavior implementation.
    type ErrorFormatter: ErrorFormatter;

    /// Type of the command-interceptor (capability-confinement) implementation.
    ///
    /// This component allows an embedding host to observe — and optionally
    /// *deny* — external command execution and file opens as they happen,
    /// in-process. See [`CommandInterceptor`] for the available hooks.
    type CommandInterceptor: CommandInterceptor;
}

/// Shell extensions implementation constructed from component types.
#[derive(Clone, Default)]
pub struct ShellExtensionsImpl<
    EF: ErrorFormatter = DefaultErrorFormatter,
    CI: CommandInterceptor = DefaultCommandInterceptor,
> {
    _marker: std::marker::PhantomData<(EF, CI)>,
}

impl<EF: ErrorFormatter, CI: CommandInterceptor> ShellExtensions for ShellExtensionsImpl<EF, CI> {
    type ErrorFormatter = EF;
    type CommandInterceptor = CI;
}

/// Default shell extensions implementation.
/// This is a type alias for the most common shell configuration.
pub type DefaultShellExtensions =
    ShellExtensionsImpl<DefaultErrorFormatter, DefaultCommandInterceptor>;

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

/// Decision returned by [`CommandInterceptor::before_exec`] to control whether
/// an external command is allowed to spawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecDecision {
    /// Allow the external command to be spawned (the default).
    Allow,
    /// Deny the external command. The contained string explains why; it is
    /// surfaced to the shell as an [`error::Error`] and the command does not
    /// run.
    Deny(String),
}

/// Decision returned by [`CommandInterceptor::before_open`] to control whether
/// a file may be opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenDecision {
    /// Allow the file to be opened (the default).
    Allow,
    /// Deny opening the file. The contained string explains why; it is surfaced
    /// to the shell as an [`error::Error`] and the file is not opened.
    Deny(String),
}

/// Trait for intercepting potentially-sensitive shell operations so an
/// embedding host can apply capability confinement (object-capability style
/// authority attenuation) *in-process*.
///
/// The default implementation ([`DefaultCommandInterceptor`]) allows
/// everything, making it byte-for-byte equivalent to a shell with no
/// interceptor at all. Embedders supply their own implementation via the
/// [`ShellExtensions::CommandInterceptor`] associated type to enforce a policy.
///
/// # Why this exists
///
/// Without these hooks, a hosting process cannot reliably confine command
/// execution in-process: a command whose name contains a path separator (e.g.
/// `/bin/rm` or `./script`) bypasses both the `PATH` search and the builtin
/// table and is executed directly. [`before_exec`](Self::before_exec) is called
/// at *every* external-spawn site — including that path-separator branch — so a
/// policy here cannot be circumvented by spelling the command differently.
pub trait CommandInterceptor: Clone + Default + Send + Sync + 'static {
    /// Called immediately before an external command is spawned, at every spawn
    /// site (including the path-separator branch that bypasses `PATH` and the
    /// builtin table). Returning [`ExecDecision::Deny`] prevents the command
    /// from running and fails it with an error.
    ///
    /// # Arguments
    ///
    /// * `program` - The program that is about to be executed. For commands
    ///   resolved via `PATH` this is the resolved absolute path; for
    ///   path-separator commands it is the path as written by the user.
    /// * `args` - The argument strings that would be passed to the program
    ///   (not including `argv[0]`).
    fn before_exec(&self, program: &str, args: &[String]) -> ExecDecision {
        let _ = (program, args);
        ExecDecision::Allow
    }

    /// Called immediately before a file is opened via a filesystem path
    /// (redirections and `source`/`.`). Returning [`OpenDecision::Deny`]
    /// prevents the file from being opened and fails the operation with an
    /// error.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path that is about to be opened.
    /// * `write` - Whether the open requests write access (`true`) or is
    ///   read-only (`false`).
    fn before_open(&self, path: &Path, write: bool) -> OpenDecision {
        let _ = (path, write);
        OpenDecision::Allow
    }
}

/// Default command-interceptor implementation: allows all execs and opens.
///
/// A shell configured with this interceptor behaves identically to a shell with
/// no interception at all.
#[derive(Clone, Default)]
pub struct DefaultCommandInterceptor;

impl CommandInterceptor for DefaultCommandInterceptor {}

/// Trait for placeholder behavior (stub for future extension).
pub trait PlaceholderBehavior: Clone + Default + Send + Sync + 'static {}

/// Default placeholder implementation.
#[derive(Clone, Default)]
pub struct DefaultPlaceholder;

impl PlaceholderBehavior for DefaultPlaceholder {}
