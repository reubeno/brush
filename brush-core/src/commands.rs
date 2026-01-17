//! Command execution and policy hook types.
//!
//! This module provides types and functions for executing shell commands, including:
//!
//! - [`SimpleCommand`]: Represents a simple command to be executed
//! - [`ExecutionContext`]: Context for command execution
//! - [`CommandArg`]: Arguments to commands
//! - [`ShellForCommand`]: Shell reference for command execution
//!
//! # Post-Expansion Policy Hook (experimental-filters)
//!
//! When the `experimental-filters` feature is enabled, this module also provides types
//! for the post-expansion policy hook system, which allows security/policy enforcement
//! against expansion-based command obfuscation:
//!
//! - [`CommandContext`]: Comprehensive context about a command after expansion
//! - [`PipelineContext`]: Position information for commands in pipelines
//! - [`RedirectionInfo`], [`RedirectionTarget`], [`RedirectionMode`]: I/O redirection details
//! - [`DispatchTarget`]: Classification of command dispatch (builtin, function, external)
//! - [`PolicyDecision`]: Hook result indicating whether to continue or return early
//!
//! ## Pre-Command-Substitution Hook
//!
//! The module also provides types for intercepting command substitutions before execution:
//!
//! - [`CommandSubstitutionContext`]: Context about a command substitution
//! - [`SubstitutionSyntax`]: Whether `$(...)` or backticks were used
//! - [`CommandSubstitutionDecision`]: How to handle the substitution
//!
//! ## Performance Characteristics
//!
//! The policy hook system is designed for near-zero overhead when unused:
//!
//! - Context construction only occurs when an extension opts in via `needs_command_context()`
//! - When no extension needs context, the overhead is a single boolean check
//! - Context types use borrowing where possible to minimize allocations
//! - Allocations are bounded by the number of redirections in a command
//!
//! ## Feature Gating
//!
//! All policy hook types are gated behind the `experimental-filters` feature flag.
//! When this feature is disabled, these types are not available and no overhead
//! is incurred.

use std::{
    borrow::Cow,
    ffi::OsStr,
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
};

#[cfg(feature = "experimental-filters")]
use std::sync::Arc;

use brush_parser::ast;
use itertools::Itertools;
use sys::commands::{CommandExt, CommandFdInjectionExt, CommandFgControlExt};

use crate::{
    ErrorKind, ExecutionControlFlow, ExecutionParameters, ExecutionResult, Shell, ShellFd,
    builtins, commands, env, error, escape, functions,
    interp::{self, Execute, ProcessGroupPolicy},
    openfiles::{self, OpenFile, OpenFiles},
    pathsearch, processes,
    results::ExecutionSpawnResult,
    sys, trace_categories, traps, variables,
};

#[cfg(feature = "experimental-filters")]
use crate::{filter, results};

//
// Post-expansion policy hook types (experimental-filters)
//

/// Context describing a command after expansion but before dispatch.
///
/// This type is used by the post-expansion policy hook to provide comprehensive
/// information about a command for security/policy decisions. It borrows from
/// the finalized `SimpleCommand` and tracks additional execution metadata.
///
/// # Lifetime
///
/// The `'a` lifetime parameter represents the lifetime of the borrowed data
/// from the expanded command (executable, arguments, raw command, redirections).
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug)]
pub struct CommandContext<'a> {
    /// The executable name (the first expanded word, `argv[0]`).
    pub executable: &'a str,

    /// The arguments list (expanded argv excluding the executable, `argv[1..]`).
    pub arguments: &'a [CommandArg],

    /// The raw command string before expansion, when available.
    ///
    /// This represents user input prior to expansion and splitting.
    /// It is `None` when the raw command cannot be preserved.
    pub raw_command: Option<Cow<'a, str>>,

    /// Pipeline information when the command is part of a pipeline.
    ///
    /// This is `None` when the command is not part of a pipeline.
    pub pipeline: Option<PipelineContext>,

    /// The list of redirections associated with the command.
    ///
    /// Redirections preserve source order. The container is owned but
    /// elements may borrow strings from the expanded redirect targets.
    pub redirections: Vec<RedirectionInfo<'a>>,

    /// The resolved dispatch target (builtin, function, or external).
    ///
    /// This classification is computed without unbounded filesystem probing.
    pub dispatch_target: DispatchTarget,
}

/// Context describing a command's position within a pipeline.
///
/// For a pipeline like `cmd1 | cmd2 | cmd3`, each command receives a
/// `PipelineContext` with its zero-indexed position and the total length.
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineContext {
    /// Zero-indexed position of the command in the pipeline.
    ///
    /// For `cmd1 | cmd2 | cmd3`:
    /// - `cmd1` has position 0
    /// - `cmd2` has position 1
    /// - `cmd3` has position 2
    pub position: usize,

    /// Total number of commands in the pipeline.
    ///
    /// For a pipeline with N commands (N > 1), this equals N for all commands.
    pub length: usize,
}

/// Structured representation of a single I/O redirection.
///
/// This type captures the destination file descriptor, what it redirects to,
/// and how the redirection operates.
///
/// # Lifetime
///
/// The `'a` lifetime parameter represents the lifetime of borrowed strings
/// in the redirection target (e.g., file paths, here-doc content).
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectionInfo<'a> {
    /// Destination file descriptor number.
    ///
    /// Common values:
    /// - 0 = stdin
    /// - 1 = stdout
    /// - 2 = stderr
    pub fd: i32,

    /// What the file descriptor redirects to.
    pub target: RedirectionTarget<'a>,

    /// How the redirection operates.
    pub mode: RedirectionMode,
}

/// What a file descriptor redirects to.
///
/// # Lifetime
///
/// The `'a` lifetime parameter represents the lifetime of borrowed strings
/// (file paths, here-doc content, here-string content).
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectionTarget<'a> {
    /// Redirect to/from a file path (expanded value).
    ///
    /// Examples: `< input.txt`, `> output.txt`, `>> log.txt`
    FilePath(Cow<'a, str>),

    /// Redirect to/from another file descriptor.
    ///
    /// Examples: `2>&1` (stderr to stdout), `1>&2` (stdout to stderr)
    FileDescriptor(i32),

    /// Here-document content.
    ///
    /// Example:
    /// ```sh
    /// cat <<EOF
    /// content here
    /// EOF
    /// ```
    HereDoc(Cow<'a, str>),

    /// Here-string content.
    ///
    /// Example: `cat <<< "hello world"`
    HereString(Cow<'a, str>),
}

/// How a redirection operates.
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectionMode {
    /// Open for reading (`<`).
    Read,

    /// Open for writing, truncate (`>`).
    Write,

    /// Open for writing, append (`>>`).
    Append,

    /// Open for both reading and writing (`<>`).
    ReadWrite,

    /// Force overwrite even with `noclobber` (`>|`).
    Clobber,
}

/// Dispatch classification for a command.
///
/// This indicates whether a command will dispatch to a builtin, function,
/// or external executable. The classification is computed without unbounded
/// filesystem probing.
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchTarget {
    /// The command dispatches to a shell builtin.
    Builtin,

    /// The command dispatches to a shell function.
    Function,

    /// The command dispatches to an external executable.
    ///
    /// If the resolved path is already known without extra filesystem probing
    /// (e.g., the executable contains a `/` and can be treated as a path),
    /// it may be included.
    External {
        /// The resolved path to the external executable, if known without
        /// additional filesystem probing.
        resolved_path: Option<PathBuf>,
    },
}

/// Policy decision returned by the post-expansion policy hook.
///
/// This type indicates whether command execution should continue or be
/// short-circuited with an early return.
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
pub enum PolicyDecision {
    /// Proceed with command dispatch/execution.
    Continue,

    /// Skip dispatch/execution and return the provided result.
    ///
    /// The shell will use this result as if the command had executed
    /// and returned it.
    Return(ExecutionResult),
}

#[cfg(feature = "experimental-filters")]
impl std::fmt::Debug for PolicyDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Continue => write!(f, "Continue"),
            Self::Return(result) => write!(f, "Return(exit_code={:?})", result.exit_code),
        }
    }
}

/// Context passed to the pre-command-substitution policy hook.
///
/// This type provides information about a command substitution before
/// its body is executed, allowing policy hooks to inspect or block
/// substitution execution.
///
/// # Lifetime
///
/// The `'a` lifetime parameter represents the lifetime of borrowed strings
/// (the raw substitution body).
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug)]
pub struct CommandSubstitutionContext<'a> {
    /// Raw source text of the substitution body, when available.
    ///
    /// This allows policy hooks to inspect the unevaluated command text.
    /// If source spans are unavailable, this is `None`.
    pub raw_body: Option<Cow<'a, str>>,

    /// The syntax used for the command substitution.
    pub syntax: SubstitutionSyntax,
}

/// The syntax form used for a command substitution.
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstitutionSyntax {
    /// Modern `$(...)` syntax.
    DollarParen,

    /// Legacy backtick `` `...` `` syntax.
    Backticks,
}

/// Decision returned by the pre-command-substitution policy hook.
///
/// This type indicates how the shell should handle a command substitution:
/// execute it normally, replace its output, or abort with an error.
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
#[derive(Debug)]
pub enum CommandSubstitutionDecision {
    /// Execute the substitution body normally.
    ///
    /// The shell will execute the command and use its output as the
    /// substitution result.
    Allow,

    /// Do not execute the substitution body; use the provided output instead.
    ///
    /// This allows policy hooks to short-circuit substitution execution
    /// and provide a replacement value.
    ReplaceOutput {
        /// The string to use as the substitution result instead of
        /// executing the command.
        output: String,
    },

    /// Abort the expansion with an error.
    ///
    /// This prevents the substitution from executing and propagates
    /// the error to the caller.
    Error(error::Error),
}

/// Converts an AST I/O redirection to a `RedirectionInfo` with an expanded target.
///
/// This function is used to capture redirections for the post-expansion policy hook.
/// It converts the AST representation to the structured `RedirectionInfo` format,
/// using the expanded target value when available.
///
/// # Arguments
///
/// * `redirect` - The AST I/O redirection to convert.
/// * `expanded_target` - The expanded target value (e.g., expanded filename).
///
/// # Returns
///
/// A `RedirectionInfo` with the expanded target, or `None` if the redirection
/// type is not supported for capture (e.g., process substitutions).
///
/// # Feature Flag
///
/// This function is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
pub fn convert_redirect_to_info(
    redirect: &ast::IoRedirect,
    expanded_target: Option<&str>,
) -> Option<RedirectionInfo<'static>> {
    match redirect {
        ast::IoRedirect::File(fd, kind, _target) => {
            let fd_num = fd.unwrap_or_else(|| get_default_fd_for_redirect_kind(kind));
            let mode = match kind {
                ast::IoFileRedirectKind::Read => RedirectionMode::Read,
                ast::IoFileRedirectKind::Write => RedirectionMode::Write,
                ast::IoFileRedirectKind::Append => RedirectionMode::Append,
                ast::IoFileRedirectKind::ReadAndWrite => RedirectionMode::ReadWrite,
                ast::IoFileRedirectKind::Clobber => RedirectionMode::Clobber,
                ast::IoFileRedirectKind::DuplicateInput
                | ast::IoFileRedirectKind::DuplicateOutput => {
                    // For fd duplication, we need to parse the target as an fd number.
                    if let Some(target_str) = expanded_target {
                        if let Ok(target_fd) = target_str.trim_end_matches('-').parse::<i32>() {
                            return Some(RedirectionInfo {
                                fd: fd_num,
                                target: RedirectionTarget::FileDescriptor(target_fd),
                                mode: if matches!(kind, ast::IoFileRedirectKind::DuplicateInput) {
                                    RedirectionMode::Read
                                } else {
                                    RedirectionMode::Write
                                },
                            });
                        }
                    }
                    return None;
                }
            };

            let target = if let Some(path) = expanded_target {
                RedirectionTarget::FilePath(Cow::Owned(path.to_string()))
            } else {
                return None;
            };

            Some(RedirectionInfo {
                fd: fd_num,
                target,
                mode,
            })
        }
        ast::IoRedirect::HereDocument(fd, here_doc) => {
            let fd_num = fd.unwrap_or(0);
            let content = if let Some(expanded) = expanded_target {
                expanded.to_string()
            } else {
                here_doc.doc.flatten()
            };

            Some(RedirectionInfo {
                fd: fd_num,
                target: RedirectionTarget::HereDoc(Cow::Owned(content)),
                mode: RedirectionMode::Read,
            })
        }
        ast::IoRedirect::HereString(fd, _word) => {
            let fd_num = fd.unwrap_or(0);
            let content = expanded_target.map(|s| s.to_string()).unwrap_or_default();

            Some(RedirectionInfo {
                fd: fd_num,
                target: RedirectionTarget::HereString(Cow::Owned(content)),
                mode: RedirectionMode::Read,
            })
        }
        ast::IoRedirect::OutputAndError(_word, append) => {
            // This redirects both stdout and stderr to the same file.
            // We represent this as a stdout redirection; the stderr part is implicit.
            let mode = if *append {
                RedirectionMode::Append
            } else {
                RedirectionMode::Write
            };

            let target = if let Some(path) = expanded_target {
                RedirectionTarget::FilePath(Cow::Owned(path.to_string()))
            } else {
                return None;
            };

            Some(RedirectionInfo {
                fd: 1, // stdout
                target,
                mode,
            })
        }
    }
}

/// Returns the default file descriptor for a given redirect kind.
#[cfg(feature = "experimental-filters")]
const fn get_default_fd_for_redirect_kind(kind: &ast::IoFileRedirectKind) -> i32 {
    match kind {
        ast::IoFileRedirectKind::Read | ast::IoFileRedirectKind::ReadAndWrite => 0,
        ast::IoFileRedirectKind::Write
        | ast::IoFileRedirectKind::Append
        | ast::IoFileRedirectKind::Clobber
        | ast::IoFileRedirectKind::DuplicateOutput => 1,
        ast::IoFileRedirectKind::DuplicateInput => 0,
    }
}

/// Encapsulates the result of waiting for a command to complete.
pub enum CommandWaitResult {
    /// The command completed.
    CommandCompleted(ExecutionResult),
    /// The command was stopped before it completed.
    CommandStopped(ExecutionResult, processes::ChildProcess),
}

/// Represents the context for executing a command.
pub struct ExecutionContext<'a> {
    /// The shell in which the command is being executed.
    pub shell: &'a mut Shell,
    /// The name of the command being executed.    
    pub command_name: String,
    /// The parameters for the execution.
    pub params: ExecutionParameters,
}

impl ExecutionContext<'_> {
    /// Returns the standard input file; usable with `write!` et al.
    pub fn stdin(&self) -> impl std::io::Read + 'static {
        self.params.stdin(self.shell)
    }

    /// Returns the standard output file; usable with `write!` et al.
    pub fn stdout(&self) -> impl std::io::Write + 'static {
        self.params.stdout(self.shell)
    }

    /// Returns the standard error file; usable with `write!` et al.
    pub fn stderr(&self) -> impl std::io::Write + 'static {
        self.params.stderr(self.shell)
    }

    /// Returns the file descriptor with the given number. Returns `None`
    /// if the file descriptor is not open.
    ///
    /// # Arguments
    ///
    /// * `fd` - The file descriptor number to retrieve.
    pub fn try_fd(&self, fd: ShellFd) -> Option<openfiles::OpenFile> {
        self.params.try_fd(self.shell, fd)
    }

    /// Iterates over all open file descriptors.
    pub fn iter_fds(&self) -> impl Iterator<Item = (ShellFd, openfiles::OpenFile)> {
        self.params.iter_fds(self.shell)
    }

    pub(crate) const fn should_cmd_lead_own_process_group(&self) -> bool {
        self.shell.options().interactive
            && matches!(
                self.params.process_group_policy,
                ProcessGroupPolicy::NewProcessGroup
            )
    }
}

/// An argument to a command.
#[derive(Clone, Debug)]
pub enum CommandArg {
    /// A simple string argument.
    String(String),
    /// An assignment/declaration; typically treated as a string, but will
    /// be specially handled by a limited set of built-in commands.
    Assignment(ast::Assignment),
}

impl Display for CommandArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Assignment(a) => write!(f, "{a}"),
        }
    }
}

impl From<String> for CommandArg {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&String> for CommandArg {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl CommandArg {
    pub(crate) fn quote_for_tracing(&self) -> Cow<'_, str> {
        match self {
            Self::String(s) => escape::quote_if_needed(s, escape::QuoteMode::SingleQuote),
            Self::Assignment(a) => {
                let mut s = a.name.to_string();
                let op = if a.append { "+=" } else { "=" };
                s.push_str(op);
                s.push_str(&escape::quote_if_needed(
                    a.value.to_string().as_str(),
                    escape::QuoteMode::SingleQuote,
                ));
                s.into()
            }
        }
    }
}

/// Encapsulates a possibly-owned reference to a `Shell` for command execution.
pub enum ShellForCommand<'a> {
    /// The command is run in the same shell as its parent; the provided
    /// mutable reference allows modifying the parent shell.
    ParentShell(&'a mut Shell),
    /// The command is run in its own owned shell (which is also provided).
    OwnedShell {
        /// The owned shell.
        target: Box<Shell>,
        /// The parent shell.
        parent: &'a mut Shell,
    },
}

impl std::ops::Deref for ShellForCommand<'_> {
    type Target = Shell;

    fn deref(&self) -> &Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

impl std::ops::DerefMut for ShellForCommand<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

/// Composes a `std::process::Command` to execute the given command. Appropriately
/// configures the command name and arguments, redirections, injected file
/// descriptors, environment variables, etc.
///
/// # Arguments
///
/// * `context` - The execution context in which the command is being composed.
/// * `command_name` - The name of the command to execute.
/// * `argv0` - The value to use for `argv[0]` (may be different from the command).
/// * `args` - The arguments to pass to the command.
/// * `empty_env` - If true, the command will be executed with an empty environment; if false, the
///   command will inherit environment variables marked as exported in the provided `Shell`.
#[allow(unused_variables, reason = "argv0 is only used on unix platforms")]
pub fn compose_std_command<S: AsRef<OsStr>>(
    context: &ExecutionContext<'_>,
    command_name: &str,
    argv0: &str,
    args: &[S],
    empty_env: bool,
) -> Result<std::process::Command, error::Error> {
    let mut cmd = std::process::Command::new(command_name);

    // Override argv[0].
    // NOTE: Not supported on all platforms.
    cmd.arg0(argv0);

    // Pass through args.
    cmd.args(args);

    // Use the shell's current working dir.
    cmd.current_dir(context.shell.working_dir());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    if !empty_env {
        for (k, v) in context.shell.env().iter_exported() {
            // NOTE: To match bash behavior, we only include exported variables
            // that are set (i.e., have a value). This means a variable that
            // shows up in `declare -p` but has no *set* value will be omitted.
            if v.value().is_set() {
                cmd.env(k.as_str(), v.value().to_cow_str(context.shell).as_ref());
            }
        }
    }

    // Add in exported functions.
    if !empty_env {
        for (func_name, registration) in context.shell.funcs().iter() {
            if registration.is_exported() {
                let var_name = std::format!("BASH_FUNC_{func_name}%%");
                let value = std::format!("() {}", registration.definition().body);
                cmd.env(var_name, value);
            }
        }
    }

    // Redirect stdin, if applicable.
    match context.try_fd(OpenFiles::STDIN_FD) {
        Some(OpenFile::Stdin(_)) | None => (),
        Some(stdin_file) => {
            let as_stdio: Stdio = stdin_file.into();
            cmd.stdin(as_stdio);
        }
    }

    // Redirect stdout, if applicable.
    match context.try_fd(OpenFiles::STDOUT_FD) {
        Some(OpenFile::Stdout(_)) | None => (),
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.into();
            cmd.stdout(as_stdio);
        }
    }

    // Redirect stderr, if applicable.
    match context.try_fd(OpenFiles::STDERR_FD) {
        Some(OpenFile::Stderr(_)) | None => {}
        Some(stderr_file) => {
            let as_stdio: Stdio = stderr_file.into();
            cmd.stderr(as_stdio);
        }
    }

    // Inject any other fds.
    let other_files = context.iter_fds().filter(|(fd, _)| {
        *fd != OpenFiles::STDIN_FD && *fd != OpenFiles::STDOUT_FD && *fd != OpenFiles::STDERR_FD
    });
    cmd.inject_fds(other_files)?;

    Ok(cmd)
}

pub(crate) async fn on_preexecute(
    cmd: &mut commands::SimpleCommand<'_>,
) -> Result<(), error::Error> {
    // See if we have a DEBUG trap handler registered; call it if we do.
    invoke_debug_trap_handler_if_registered(&mut cmd.shell, &cmd.params, cmd.args.as_slice())
        .await?;

    Ok(())
}

async fn invoke_debug_trap_handler_if_registered(
    shell: &mut Shell,
    params: &ExecutionParameters,
    args: &[CommandArg],
) -> Result<(), error::Error> {
    if shell.call_stack().trap_handler_depth() > 0 {
        return Ok(());
    }

    let Some(debug_trap_handler) = shell.traps().get_handler(traps::TrapSignal::Debug).cloned()
    else {
        return Ok(());
    };

    // TODO(traps): Confirm whether trap handlers should be executed in the same process
    // group.
    let mut handler_params = params.clone();
    handler_params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    let full_cmd = args.iter().map(|arg| arg.to_string()).join(" ");

    // TODO(well-known-vars): This shouldn't *just* be set in a trap situation.
    shell.env_mut().update_or_add(
        "BASH_COMMAND",
        variables::ShellValueLiteral::Scalar(full_cmd),
        |_| Ok(()),
        env::EnvironmentLookup::Anywhere,
        env::EnvironmentScope::Global,
    )?;

    shell.enter_trap_handler(Some(&debug_trap_handler));

    // TODO(traps): Discard result?
    let _ = shell
        .run_string(
            &debug_trap_handler.command,
            &debug_trap_handler.source_info,
            &handler_params,
        )
        .await;

    shell.leave_trap_handler();

    Ok(())
}

/// Represents a simple command to be executed.
pub struct SimpleCommand<'a> {
    /// The shell to run the command in.
    shell: ShellForCommand<'a>,

    /// The execution parameters for the command.
    pub params: ExecutionParameters,

    /// The name of the command to execute.
    pub command_name: String,

    /// The arguments to the command, including the command itself.
    pub args: Vec<CommandArg>,

    /// Whether to consider shell functions when looking up the command name.
    /// If true, shell functions will be checked; if false, they will be ignored.
    pub use_functions: bool,

    /// Optional list of directories to search for external commands. If left
    /// `None`, the default search logic will be used.
    pub path_dirs: Option<Vec<String>>,

    /// The process group ID to use for externally executed commands. This may be
    /// `None`, in which case the default behavior will be used.
    pub process_group_id: Option<i32>,

    /// Pipeline context for the post-expansion policy hook.
    ///
    /// This is `Some` when the command is part of a pipeline with N > 1 commands,
    /// and `None` when the command is not part of a pipeline (single command).
    #[cfg(feature = "experimental-filters")]
    pub pipeline_info: Option<PipelineContext>,

    /// Raw pipeline text for policy hooks.
    ///
    /// When available, this contains the user-entered pipeline string (prior to
    /// expansion) so filters can inspect the full pipeline layout.
    #[cfg(feature = "experimental-filters")]
    pub raw_command: Option<Arc<str>>,

    /// Captured redirections for the post-expansion policy hook.
    ///
    /// These are the redirections associated with the command, converted from
    /// AST form with expanded targets where available.
    #[cfg(feature = "experimental-filters")]
    pub redirections: Vec<RedirectionInfo<'static>>,

    /// Optionally provides a function that can run after execution occurs. Note
    /// that it is *not* invoked if the shell is discarded during the execution
    /// process.
    #[allow(clippy::type_complexity)]
    pub post_execute: Option<fn(&mut Shell) -> Result<(), error::Error>>,
}

/// Internal dispatch information used during command execution.
///
/// This enum captures the result of dispatch target determination,
/// including the actual registration objects needed for execution.
enum DispatchInfo {
    /// A special builtin in POSIX mode (takes precedence over functions).
    SpecialBuiltin(builtins::Registration),
    /// A shell function.
    Function(functions::Registration),
    /// A regular builtin.
    Builtin(builtins::Registration),
    /// An external command (path resolution happens later).
    External,
}

impl<'a> SimpleCommand<'a> {
    /// Creates a new `SimpleCommand` instance.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell in which to execute the command.
    /// * `params` - The execution parameters for the command.
    /// * `command_name` - The name of the command to execute.
    /// * `args` - The arguments to the command, including the command itself.
    pub const fn new(
        shell: ShellForCommand<'a>,
        params: ExecutionParameters,
        command_name: String,
        args: Vec<CommandArg>,
    ) -> Self {
        Self {
            shell,
            params,
            command_name,
            args,
            use_functions: true,
            path_dirs: None,
            process_group_id: None,
            #[cfg(feature = "experimental-filters")]
            pipeline_info: None,
            #[cfg(feature = "experimental-filters")]
            raw_command: None,
            #[cfg(feature = "experimental-filters")]
            redirections: vec![],
            post_execute: None,
        }
    }

    /// Determines the dispatch target for this command.
    ///
    /// This method checks builtins, functions, and external commands in the
    /// appropriate order based on shell options (e.g., POSIX mode).
    fn determine_dispatch_target(&self) -> DispatchInfo {
        // First see if it's the name of a builtin.
        let builtin = self.shell.builtins().get(&self.command_name).cloned();

        // If we're in POSIX mode and found a special builtin (that's not disabled),
        // then it takes precedence over functions.
        if self.shell.options().posix_mode
            && builtin
                .as_ref()
                .is_some_and(|r| !r.disabled && r.special_builtin)
        {
            #[allow(clippy::unwrap_used, reason = "we just checked that builtin is Some")]
            return DispatchInfo::SpecialBuiltin(builtin.unwrap());
        }

        // Assuming we weren't requested not to do so, check if it's the name of
        // a shell function.
        if self.use_functions {
            if let Some(func_registration) =
                self.shell.funcs().get(self.command_name.as_str()).cloned()
            {
                return DispatchInfo::Function(func_registration);
            }
        }

        // If we haven't yet resolved the command name and found a builtin that's not disabled,
        // then use it.
        if let Some(builtin) = builtin {
            if !builtin.disabled {
                return DispatchInfo::Builtin(builtin);
            }
        }

        // External command (path resolution happens later).
        DispatchInfo::External
    }

    /// Builds a `CommandContext` for the post-expansion policy hook.
    ///
    /// This method constructs the context with information about the command,
    /// including the executable, arguments, and dispatch target classification.
    #[cfg(feature = "experimental-filters")]
    fn build_command_context(&self, dispatch_info: &DispatchInfo) -> CommandContext<'_> {
        // Extract executable and arguments from args.
        let (executable, arguments) = if let Some(CommandArg::String(exe)) = self.args.first() {
            (exe.as_str(), &self.args[1..])
        } else {
            // Fallback to command_name if args is empty (shouldn't happen in practice).
            (self.command_name.as_str(), &self.args[..])
        };

        // Convert dispatch info to dispatch target for the context.
        let dispatch_target = match dispatch_info {
            DispatchInfo::SpecialBuiltin(_) | DispatchInfo::Builtin(_) => DispatchTarget::Builtin,
            DispatchInfo::Function(_) => DispatchTarget::Function,
            DispatchInfo::External => {
                // Check if the command name contains a path separator.
                // If so, we can include it as the resolved path without extra probing.
                let resolved_path = if self.command_name.contains(std::path::MAIN_SEPARATOR) {
                    Some(PathBuf::from(&self.command_name))
                } else {
                    None
                };
                DispatchTarget::External { resolved_path }
            }
        };

        // Convert captured redirections to borrowed form for the context.
        let redirections: Vec<RedirectionInfo<'_>> = self
            .redirections
            .iter()
            .map(|r| RedirectionInfo {
                fd: r.fd,
                target: match &r.target {
                    RedirectionTarget::FilePath(p) => RedirectionTarget::FilePath(Cow::Borrowed(p)),
                    RedirectionTarget::FileDescriptor(fd) => RedirectionTarget::FileDescriptor(*fd),
                    RedirectionTarget::HereDoc(content) => {
                        RedirectionTarget::HereDoc(Cow::Borrowed(content))
                    }
                    RedirectionTarget::HereString(content) => {
                        RedirectionTarget::HereString(Cow::Borrowed(content))
                    }
                },
                mode: r.mode,
            })
            .collect();

        CommandContext {
            executable,
            arguments,
            raw_command: self.raw_command.as_deref().map(Cow::Borrowed),
            pipeline: self.pipeline_info, // Pipeline info from the execution context.
            redirections,
            dispatch_target,
        }
    }

    /// Executes the simple command, applying any registered filters.
    #[allow(
        unused_mut,
        reason = "mut needed when experimental-filters feature is enabled"
    )]
    pub async fn execute(mut self) -> Result<ExecutionSpawnResult, error::Error> {
        crate::with_filter!(
            self.shell,
            pre_exec_simple_command,
            post_exec_simple_command,
            self,
            |cmd| cmd.execute_impl().await
        )
    }

    /// Executes the simple command.
    ///
    /// The command may be a builtin, a shell function, or an externally
    /// executed command. This function's implementation is responsible for
    /// dispatching it appropriately according to the context provided.
    async fn execute_impl(mut self) -> Result<ExecutionSpawnResult, error::Error> {
        // Determine the dispatch target for this command.
        let dispatch_info = self.determine_dispatch_target();

        // Check if any extension needs command context for the post-expansion policy hook.
        #[cfg(feature = "experimental-filters")]
        if self.shell.extensions().needs_command_context() {
            // Build the command context for policy enforcement.
            let context = self.build_command_context(&dispatch_info);

            // Invoke the post-expansion policy hook.
            match self
                .shell
                .extensions()
                .post_expansion_pre_dispatch_simple_command(&self, &context)
            {
                PolicyDecision::Continue => {
                    // Continue with dispatch.
                }
                PolicyDecision::Return(result) => {
                    // Skip dispatch and return the provided result.
                    if let Some(post_execute) = self.post_execute {
                        let _ = post_execute(&mut self.shell);
                    }
                    return Ok(result.into());
                }
            }
        }

        // Dispatch based on the determined target.
        match dispatch_info {
            DispatchInfo::SpecialBuiltin(builtin) => {
                return self.execute_via_builtin(builtin).await;
            }
            DispatchInfo::Function(func_registration) => {
                return self.execute_via_function(func_registration).await;
            }
            DispatchInfo::Builtin(builtin) => {
                return self.execute_via_builtin(builtin).await;
            }
            DispatchInfo::External => {
                // Fall through to external command handling below.
            }
        }

        // We still haven't found a command to invoke. We'll need to look for an external command.
        if !self.command_name.contains(std::path::MAIN_SEPARATOR) {
            // All else failed; if we were given path directories to search, try to look through
            // them for a matching executable. Otherwise, use our default search logic.
            let path = if let Some(path_dirs) = &self.path_dirs {
                pathsearch::search_for_executable(
                    path_dirs.iter().map(String::as_str),
                    self.command_name.as_str(),
                )
                .next()
            } else {
                self.shell
                    .find_first_executable_in_path_using_cache(&self.command_name)
            };

            if let Some(path) = path {
                self.execute_via_external(&path).await
            } else {
                if let Some(post_execute) = self.post_execute {
                    let _ = post_execute(&mut self.shell);
                }

                Err(ErrorKind::CommandNotFound(self.command_name).into())
            }
        } else {
            let command_name = PathBuf::from(self.command_name.clone());
            self.execute_via_external(command_name.as_path()).await
        }
    }

    async fn execute_via_builtin(
        self,
        builtin: builtins::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        match self.shell {
            ShellForCommand::OwnedShell { target, .. } => {
                Ok(Self::execute_via_builtin_in_owned_shell(
                    *target,
                    self.params,
                    builtin,
                    self.command_name,
                    self.args,
                ))
            }
            ShellForCommand::ParentShell(..) => {
                self.execute_via_builtin_in_parent_shell(builtin).await
            }
        }
    }

    fn execute_via_builtin_in_owned_shell(
        mut shell: Shell,
        params: ExecutionParameters,
        builtin: builtins::Registration,
        command_name: String,
        args: Vec<CommandArg>,
    ) -> ExecutionSpawnResult {
        let join_handle = tokio::task::spawn_blocking(move || {
            let cmd_context = ExecutionContext {
                shell: &mut shell,
                command_name,
                params,
            };

            let rt = tokio::runtime::Handle::current();
            rt.block_on(execute_builtin_command(&builtin, cmd_context, args))
        });

        ExecutionSpawnResult::StartedTask(join_handle)
    }

    async fn execute_via_builtin_in_parent_shell(
        self,
        builtin: builtins::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let result = execute_builtin_command(&builtin, cmd_context, self.args).await;

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        let result = result?;

        Ok(result.into())
    }

    async fn execute_via_function(
        self,
        func_registration: functions::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        // Strip the function name off args.
        let result = invoke_shell_function(func_registration, cmd_context, &self.args[1..]).await;

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        result
    }

    async fn execute_via_external(self, path: &Path) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let resolved_path = path.to_string_lossy();
        let result = execute_external_command(
            cmd_context,
            resolved_path.as_ref(),
            self.process_group_id,
            &self.args[1..],
        )
        .await;

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        result
    }
}

#[cfg(feature = "experimental-filters")]
impl filter::FilterableOp for commands::SimpleCommand<'_> {
    type Input = Self;
    type Output = Result<results::ExecutionSpawnResult, error::Error>;
}

#[expect(
    clippy::unused_async,
    reason = "Async needed when experimental-filters feature is enabled"
)]
pub(crate) async fn execute_external_command(
    context: ExecutionContext<'_>,
    executable_path: &str,
    process_group_id: Option<i32>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    // Filter out the args; we only want strings.
    let mut cmd_args = vec![];
    for arg in args {
        if let CommandArg::String(s) = arg {
            cmd_args.push(s);
        }
    }

    // Before we lose ownership of the open files, figure out if stdin will be a terminal.
    let child_stdin_is_terminal = context
        .try_fd(openfiles::OpenFiles::STDIN_FD)
        .is_some_and(|f| f.is_terminal());

    // Figure out if we should be setting up a new process group.
    let new_pg = context.should_cmd_lead_own_process_group();

    // Compose the std::process::Command that encapsulates what we want to launch.
    #[allow(unused_mut, reason = "only mutated on unix platforms")]
    let mut cmd = compose_std_command(
        &context,
        executable_path,
        context.command_name.as_str(),
        cmd_args.as_slice(),
        false, /* empty environment? */
    )?;

    // Set up process group state.
    if new_pg {
        // Check if we'll be doing terminal control setup (which includes setsid)
        if child_stdin_is_terminal && context.shell.options().external_cmd_leads_session {
            // Don't set process_group(0) - setsid() in pre_exec will handle it
            cmd.lead_session();
        } else {
            // Normal case: create new process group in current session
            cmd.process_group(0);
            if child_stdin_is_terminal {
                cmd.take_foreground();
            }
        }
    } else {
        // We need to join an established process group.
        if let Some(pgid) = process_group_id {
            cmd.process_group(pgid);
        }
    }

    // When tracing is enabled, report.
    tracing::debug!(
        target: trace_categories::COMMANDS,
        "Spawning: cmd='{} {}'",
        cmd.get_program().to_string_lossy().to_string(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .join(" ")
    );

    let spawn_result = crate::with_filter!(
        context.shell,
        pre_exec_external_command,
        post_exec_external_command,
        cmd,
        |cmd| sys::process::spawn(cmd)
    );

    match spawn_result {
        Ok(child) => {
            // Retrieve the pid.
            #[expect(clippy::cast_possible_wrap)]
            let pid = child.id().map(|id| id as i32);
            let mut actual_pgid = process_group_id;
            if let Some(pid) = &pid {
                if new_pg {
                    actual_pgid = Some(*pid);
                }
            } else {
                tracing::warn!("could not retrieve pid for child process");
            }

            Ok(ExecutionSpawnResult::StartedProcess(
                processes::ChildProcess::new(child, pid, actual_pgid),
            ))
        }
        Err(spawn_err) => {
            if context.shell.options().interactive {
                sys::terminal::move_self_to_foreground()?;
            }

            if spawn_err.kind() == std::io::ErrorKind::NotFound {
                if !context.shell.working_dir().exists() {
                    Err(
                        error::ErrorKind::WorkingDirMissing(context.shell.working_dir().to_owned())
                            .into(),
                    )
                } else {
                    Err(error::ErrorKind::CommandNotFound(context.command_name).into())
                }
            } else {
                Err(
                    error::ErrorKind::FailedToExecuteCommand(context.command_name, spawn_err)
                        .into(),
                )
            }
        }
    }
}

/// Marker type for external command execution filtering.
///
/// This type defines the input/output signature for filtering external
/// command spawns. It does not contain execution logic; that is provided
/// at the call site.
///
/// # Feature Flag
///
/// This type is only available when the `experimental-filters` feature is enabled.
#[cfg(feature = "experimental-filters")]
pub struct ExecuteExternalCommand;

#[cfg(feature = "experimental-filters")]
impl filter::FilterableOp for ExecuteExternalCommand {
    type Input = std::process::Command;
    type Output = Result<sys::process::Child, std::io::Error>;
}

async fn execute_builtin_command(
    builtin: &builtins::Registration,
    context: ExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> Result<ExecutionResult, error::Error> {
    // In POSIX mode, special builtins that return errors are to be treated as fatal.
    let mark_errors_fatal = builtin.special_builtin && context.shell.options().posix_mode;

    (builtin.execute_func)(context, args)
        .await
        .map_err(|e| if mark_errors_fatal { e.into_fatal() } else { e })
}

pub(crate) async fn invoke_shell_function(
    function: functions::Registration,
    mut context: ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    let ast::FunctionBody(body, redirects) = &function.definition().body;

    // Apply any redirects specified at function definition-time.
    if let Some(redirects) = redirects {
        for redirect in &redirects.0 {
            interp::setup_redirect(context.shell, &mut context.params, redirect).await?;
        }
    }

    let positional_args = args.iter().map(|a| a.to_string());

    // Pass through open files.
    let params = context.params.clone();

    // Note that we're going deeper. Once we do this, we need to make sure we don't bail early
    // before "exiting" the function.
    context.shell.enter_function(
        context.command_name.as_str(),
        &function,
        positional_args,
        &context.params,
    )?;

    // Invoke the function.
    let result = body.execute(context.shell, &params).await;

    // Clean up parameters so any owned files are closed.
    drop(params);

    // We've come back out, reflect it.
    context.shell.leave_function()?;

    // Get the actual execution result from the body of the function.
    let mut result = result?;

    // Handle control-flow.
    match result.next_control_flow {
        ExecutionControlFlow::BreakLoop { .. } | ExecutionControlFlow::ContinueLoop { .. } => {
            return error::unimp("break or continue returned from function invocation");
        }
        ExecutionControlFlow::ReturnFromFunctionOrScript => {
            // It's now been handled.
            result.next_control_flow = ExecutionControlFlow::Normal;
        }
        _ => {}
    }

    Ok(result.into())
}

pub(crate) async fn invoke_command_in_subshell_and_get_output(
    shell: &mut Shell,
    params: &ExecutionParameters,
    s: String,
) -> Result<String, error::Error> {
    // Instantiate a subshell to run the command in.
    let mut subshell = shell.clone();

    // Command substitutions don't inherit errexit by default. Only inherit it when
    // command_subst_inherits_errexit is enabled, otherwise disable errexit in the subshell.
    if !shell.options().command_subst_inherits_errexit {
        subshell.options_mut().exit_on_nonzero_command_exit = false;
    }

    // Get our own set of parameters we can customize and use.
    let mut params = params.clone();
    params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    // Set up pipe so we can read the output.
    let (reader, writer) = std::io::pipe()?;
    params.set_fd(OpenFiles::STDOUT_FD, writer.into());

    // Start the execution of the command, but don't wait for it to
    // complete. In case the command generates lots of output, we
    // need to start reading in parallel so the command doesn't block
    // when the pipe's buffer fills up. We pass ownership of the
    // subshell and params to run_substitution_command; we must
    // ensure that they're both dropped by the time this call
    // returns (so they're not holding onto the write end of the pipe).
    let cmd_join_handle = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(run_substitution_command(subshell, params, s))
    });

    // Extract output.
    let output_str = std::io::read_to_string(reader)?;

    // Now observe the command's completion.
    let run_result = cmd_join_handle.await?;
    let cmd_result = run_result?;

    // Store the status.
    shell.set_last_exit_status(cmd_result.exit_code.into());

    Ok(output_str)
}

async fn run_substitution_command(
    mut shell: Shell,
    mut params: ExecutionParameters,
    command: String,
) -> Result<ExecutionResult, error::Error> {
    // Parse the string into a whole shell program.
    let parse_result = shell.parse_string(command);

    // Check for a command that is only an input redirection ("< file").
    // If detected, emulate `cat file` to stdout and return immediately.
    // If we failed to parse, then we'll fall below and handle it there.
    if let Ok(program) = &parse_result {
        if let Some(redir) = try_unwrap_bare_input_redir_program(program) {
            interp::setup_redirect(&mut shell, &mut params, redir).await?;
            std::io::copy(&mut params.stdin(&shell), &mut params.stdout(&shell))?;
            return Ok(ExecutionResult::new(0));
        }
    }

    // TODO(source-info): review this
    let source_info = crate::SourceInfo::from("main");

    // Handle the parse result using default shell behavior.
    shell
        .run_parsed_result(parse_result, &source_info, &params)
        .await
}

// Detects a subshell command that consists solely of a single input redirection
// (e.g., "< file"), returning the IoRedirect when present.
fn try_unwrap_bare_input_redir_program(program: &ast::Program) -> Option<&ast::IoRedirect> {
    // We're looking for exactly one complete command...
    let [complete] = program.complete_commands.as_slice() else {
        return None;
    };

    // ...a single list item...
    let ast::CompoundList(items) = complete;
    let [item] = items.as_slice() else {
        return None;
    };

    // ...with a single pipeline (no && or || chaining)...
    let and_or = &item.0;
    if !and_or.additional.is_empty() {
        return None;
    }

    // ...not negated...
    let pipeline = &and_or.first;
    if pipeline.bang {
        return None;
    }

    // ...with a single command in the pipeline...
    let [ast::Command::Simple(simple_cmd)] = pipeline.seq.as_slice() else {
        return None;
    };

    // ...with no program word/name and no suffix...
    if simple_cmd.word_or_name.is_some() || simple_cmd.suffix.is_some() {
        return None;
    }

    // ...and exactly one prefix containing an I/O redirect...
    let prefix = simple_cmd.prefix.as_ref()?;
    let [ast::CommandPrefixOrSuffixItem::IoRedirect(redir)] = prefix.0.as_slice() else {
        return None;
    };

    // ...that is a file input redirection to a filename, targeting stdin.
    match redir {
        ast::IoRedirect::File(
            fd,
            ast::IoFileRedirectKind::Read,
            ast::IoFileRedirectTarget::Filename(..),
        ) if fd.is_none_or(|fd| fd == openfiles::OpenFiles::STDIN_FD) => Some(redir),
        _ => None,
    }
}
