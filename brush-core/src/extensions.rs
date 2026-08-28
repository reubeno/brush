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

/// The access an open request is asking for.
///
/// This is the shell's *declared* intent for the open, derived at the call site
/// (e.g. from the [`brush_parser::ast::IoFileRedirectKind`] of a redirection),
/// not reverse-engineered from the resulting [`std::fs::OpenOptions`]. It is the
/// axis a confinement policy selects on: read authority versus write authority.
///
/// [`brush_parser::ast::IoFileRedirectKind`]: ../../brush_parser/ast/enum.IoFileRedirectKind.html
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAccess {
    /// The file is opened for reading only (`< file`, `source`/`.`).
    Read,
    /// The file is opened for writing only, whether by truncation or append
    /// (`> file`, `>> file`, `>| file`, `&> file`).
    Write,
    /// The file is opened for both reading and writing (`<> file`).
    ReadWrite,
}

impl OpenAccess {
    /// Returns whether this access grants the ability to read the file.
    #[must_use]
    pub const fn reads(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    /// Returns whether this access grants the ability to modify the file.
    #[must_use]
    pub const fn writes(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

/// Describes a file open that the shell is about to perform, as presented to
/// [`CommandInterceptor::before_open`].
///
/// The struct is `#[non_exhaustive]` so that further detail can be added later
/// without breaking implementors; construct one with [`OpenRequest::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpenRequest<'a> {
    /// The absolute path that is about to be opened. Already resolved against
    /// the shell's working directory, but *not* canonicalized — a policy that
    /// cares about symlink or `..` escapes must canonicalize it itself.
    pub path: &'a Path,
    /// The access the shell is requesting.
    pub access: OpenAccess,
}

impl<'a> OpenRequest<'a> {
    /// Creates a new open request.
    ///
    /// # Arguments
    ///
    /// * `path` - The absolute path about to be opened.
    /// * `access` - The access being requested.
    #[must_use]
    pub const fn new(path: &'a Path, access: OpenAccess) -> Self {
        Self { path, access }
    }
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
    /// * `request` - What is about to be opened, and with what access. The
    ///   access is the shell's declared intent, taken from the syntax that
    ///   requested the open; see [`OpenRequest`].
    ///
    /// # Coverage
    ///
    /// This fires for every path-based open the shell performs. One documented
    /// exception exists: platform-specific special files are resolved before
    /// the path is made absolute, and are not shown to this hook. Today that is
    /// only `/dev/null` on Windows (mapped to `NUL`); on Unix the special-file
    /// hook resolves nothing at all. Opens performed by an already-spawned
    /// external process are outside the shell entirely and never reach here.
    fn before_open(&self, request: &OpenRequest<'_>) -> OpenDecision {
        let _ = request;
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
