//! Shell extensions support.
//!
//! This module provides the [`ShellExtensions`] trait for extending core shell behaviors.
//! Extensions can intercept and modify shell operations at key execution points.
//!
//! # Default Implementation
//!
//! The [`DefaultExtensions`] type provides a zero-overhead default implementation
//! that passes through all operations unchanged.
//!
//! # Post-Expansion Policy Hook (experimental-filters)
//!
//! When the `experimental-filters` feature is enabled, extensions can implement
//! policy enforcement hooks:
//!
//! - [`ShellExtensions::needs_command_context`]: Opt-in to receive command context
//! - [`ShellExtensions::post_expansion_pre_dispatch_simple_command`]: Policy hook for commands
//! - [`ShellExtensions::pre_command_substitution`]: Policy hook for command substitutions
//!
//! ## Performance Characteristics
//!
//! The extension system is designed for minimal overhead:
//!
//! - When `needs_command_context()` returns `false` (the default), no context is constructed
//! - The overhead when no extension needs context is a single boolean check
//! - Extensions are cloned for subshells; use `Arc` for shared state to avoid duplication
//!
//! ## Subshell Cloning
//!
//! Extensions must implement [`ShellExtensions::clone_for_subshell`] to support subshell
//! creation. For stateful extensions, use `Arc<Mutex<T>>` or similar shared ownership
//! patterns to maintain consistent state across parent and child shells.
//!
//! # Example
//!
//! ```ignore
//! use std::sync::Arc;
//! use brush_core::extensions::ShellExtensions;
//!
//! #[derive(Clone)]
//! struct MyExtensions {
//!     // Use Arc for shared state across subshells
//!     shared_state: Arc<MyState>,
//! }
//!
//! impl ShellExtensions for MyExtensions {
//!     fn clone_for_subshell(&self) -> Box<dyn ShellExtensions> {
//!         Box::new(self.clone())
//!     }
//!
//!     #[cfg(feature = "experimental-filters")]
//!     fn needs_command_context(&self) -> bool {
//!         true // Opt-in to receive command context
//!     }
//! }
//! ```

use crate::error;
use crate::shell::Shell;

#[cfg(feature = "experimental-filters")]
use crate::{ExecutionResult, commands, expansion, filter, shell};

#[cfg(feature = "experimental-filters")]
/// Marker type for source script filtering.
///
/// This type defines the input/output signature for filtering script
/// sourcing operations.
#[derive(Debug)]
pub struct SourceScriptOp<'a> {
    marker: std::marker::PhantomData<&'a ()>,
}

#[cfg(feature = "experimental-filters")]
impl<'a> crate::filter::FilterableOp for SourceScriptOp<'a> {
    type Input = shell::ScriptArgs<'a>;
    type Output = Result<ExecutionResult, error::Error>;
}

/// Trait for extending core shell behaviors.
///
/// This trait allows clients to handle, and in some cases, intercept and modify
/// shell operations at key points during execution. All methods have default
/// implementations.
///
/// Implementations should use interior mutability (`Arc<Mutex<T>>`, etc.) to share
/// mutable state.
pub trait ShellExtensions: Send + Sync {
    /// Format an error to generate a string displayable to a user.
    ///
    /// # Arguments
    ///
    /// * `err` - The error to format.
    /// * `_shell` - The shell instance where the error occurred.
    fn format_error(&self, err: &error::Error, shell: &Shell) -> String {
        let _ = shell;
        std::format!("error: {err:#}\n")
    }

    /// Indicates whether this extension needs command context for the
    /// post-expansion policy hook.
    ///
    /// When this returns `true`, the shell will construct a [`commands::CommandContext`]
    /// for each simple command and invoke [`Self::post_expansion_pre_dispatch_simple_command`].
    ///
    /// When no registered extension returns `true`, the context is not constructed
    /// and the post-expansion policy hook is not invoked, ensuring near-zero overhead.
    ///
    /// # Returns
    ///
    /// `false` by default. Override to return `true` if your extension needs
    /// to inspect command context for policy decisions.
    #[cfg(feature = "experimental-filters")]
    fn needs_command_context(&self) -> bool {
        false
    }

    /// Called after expansion but before dispatch for policy enforcement.
    ///
    /// This hook is invoked once per simple command after word expansion and after
    /// the existing [`Self::pre_exec_simple_command`] filter chain has produced the
    /// final command, but before dispatch to builtins, functions, or external executables.
    ///
    /// Unlike [`Self::pre_exec_simple_command`], this hook is **read-only** with respect
    /// to the command. It is intended for policy enforcement (allow/deny decisions) based
    /// on the de-obfuscated argv and additional context (raw command, redirections,
    /// pipeline position, dispatch target).
    ///
    /// Extensions that want to modify commands should do so in [`Self::pre_exec_simple_command`]
    /// and use this hook purely for allow/deny decisions.
    ///
    /// # Arguments
    ///
    /// * `command` - The simple command about to be dispatched (read-only).
    /// * `context` - Comprehensive context about the command including executable,
    ///   arguments, raw command, pipeline position, redirections, and dispatch target.
    ///
    /// # Returns
    ///
    /// A [`commands::PolicyDecision`] indicating whether to continue with dispatch
    /// or return early with a custom result:
    /// - [`commands::PolicyDecision::Continue`]: Proceed with command dispatch/execution.
    /// - [`commands::PolicyDecision::Return`]: Skip dispatch and return the provided result.
    ///
    /// # Invocation Rules
    ///
    /// - Only extensions with [`Self::needs_command_context`] returning `true` are invoked.
    /// - Invocation order is deterministic (registration order).
    /// - First "return early" wins; subsequent extensions are not invoked.
    ///
    /// # Feature Flag
    ///
    /// This method is only available when the `experimental-filters` feature is enabled.
    #[cfg(feature = "experimental-filters")]
    fn post_expansion_pre_dispatch_simple_command(
        &self,
        _command: &commands::SimpleCommand<'_>,
        _context: &commands::CommandContext<'_>,
    ) -> commands::PolicyDecision {
        commands::PolicyDecision::Continue
    }

    /// Called before a simple command is executed.
    ///
    /// This can intercept and modify commands before they are dispatched
    /// to builtins, functions, or external executables.
    ///
    /// # Arguments
    ///
    /// * `input` - The command about to be executed.
    ///
    /// # Returns
    ///
    /// A [`filter::PreFilterResult`] indicating whether to continue with execution
    /// or return early with a custom result.
    #[cfg(feature = "experimental-filters")]
    fn pre_exec_simple_command<'a>(
        &self,
        input: commands::SimpleCommand<'a>,
    ) -> filter::PreFilterResult<commands::SimpleCommand<'a>> {
        filter::PreFilterResult::Continue(input)
    }

    /// Called after a simple command is executed.
    ///
    /// # Arguments
    ///
    /// * `output` - The result of command execution.
    ///
    /// # Returns
    ///
    /// A [`filter::PostFilterResult`] with the (possibly modified) result.
    #[cfg(feature = "experimental-filters")]
    fn post_exec_simple_command<'a>(
        &self,
        output: <commands::SimpleCommand<'a> as filter::FilterableOp>::Output,
    ) -> filter::PostFilterResult<commands::SimpleCommand<'a>> {
        filter::PostFilterResult::Return(output)
    }

    /// Called before an external command is spawned.
    ///
    /// This can intercept the final `std::process::Command` before it is
    /// spawned as a child process.
    ///
    /// # Arguments
    ///
    /// * `input` - The process command about to be spawned.
    ///
    /// # Returns
    ///
    /// A [`filter::PreFilterResult`] indicating whether to continue with spawning
    /// or return early with a custom result.
    #[cfg(feature = "experimental-filters")]
    fn pre_exec_external_command(
        &self,
        input: std::process::Command,
    ) -> filter::PreFilterResult<commands::ExecuteExternalCommand> {
        filter::PreFilterResult::Continue(input)
    }

    /// Called after an external command is spawned.
    ///
    /// # Arguments
    ///
    /// * `output` - The result of spawning the process.
    ///
    /// # Returns
    ///
    /// A [`filter::PostFilterResult`] with the (possibly modified) result.
    #[cfg(feature = "experimental-filters")]
    fn post_exec_external_command(
        &self,
        output: <commands::ExecuteExternalCommand as filter::FilterableOp>::Output,
    ) -> filter::PostFilterResult<commands::ExecuteExternalCommand> {
        filter::PostFilterResult::Return(output)
    }

    /// Called before a word is expanded.
    ///
    /// This can intercept and modify word expansion operations, including
    /// parameter expansion, command substitution, and arithmetic expansion.
    ///
    /// # Arguments
    ///
    /// * `input` - The word about to be expanded.
    ///
    /// # Returns
    ///
    /// A [`filter::PreFilterResult`] indicating whether to continue with expansion
    /// or return early with a custom result.
    #[cfg(feature = "experimental-filters")]
    fn pre_expand_word<'a>(
        &self,
        input: &'a str,
    ) -> filter::PreFilterResult<expansion::ExpandWordOp<'a>> {
        filter::PreFilterResult::Continue(input)
    }

    /// Called after a word is expanded.
    ///
    /// # Arguments
    ///
    /// * `output` - The result of word expansion.
    ///
    /// # Returns
    ///
    /// A [`filter::PostFilterResult`] with the (possibly modified) result.
    #[cfg(feature = "experimental-filters")]
    fn post_expand_word<'a>(
        &self,
        output: <expansion::ExpandWordOp<'a> as filter::FilterableOp>::Output,
    ) -> filter::PostFilterResult<expansion::ExpandWordOp<'a>> {
        filter::PostFilterResult::Return(output)
    }

    /// Called before a script is sourced.
    ///
    /// This can intercept when a script is sourced (e.g., via the `.` or `source` builtins).
    ///
    /// # Arguments
    ///
    /// * `input` - The script arguments (path and parameters).
    ///
    /// # Returns
    ///
    /// A [`filter::PreFilterResult`] indicating whether to continue with sourcing
    /// or return early with a custom result.
    #[cfg(feature = "experimental-filters")]
    fn pre_source_script<'a>(
        &self,
        input: shell::ScriptArgs<'a>,
    ) -> filter::PreFilterResult<SourceScriptOp<'a>> {
        filter::PreFilterResult::Continue(input)
    }

    /// Called after a script is sourced.
    ///
    /// # Arguments
    ///
    /// * `output` - The result of script execution.
    ///
    /// # Returns
    ///
    /// A [`filter::PostFilterResult`] with the (possibly modified) result.
    #[cfg(feature = "experimental-filters")]
    fn post_source_script<'a>(
        &self,
        output: <SourceScriptOp<'a> as filter::FilterableOp>::Output,
    ) -> filter::PostFilterResult<SourceScriptOp<'a>> {
        filter::PostFilterResult::Return(output)
    }

    /// Called before a command substitution is executed.
    ///
    /// This hook is invoked before executing both `$(...)` and backtick (`` `...` ``)
    /// command substitutions during word expansion. It allows policy enforcement to
    /// prevent side effects that would otherwise occur during expansion.
    ///
    /// # Security Context
    ///
    /// Command substitution executes during expansion, which means a post-expansion
    /// hook alone cannot prevent side effects inside substitutions. This hook provides
    /// an explicit gate before executing substitution bodies.
    ///
    /// # Arguments
    ///
    /// * `context` - Context about the command substitution including the raw body
    ///   text (when available) and the syntax form used (`$(...)` or backticks).
    ///
    /// # Returns
    ///
    /// A [`commands::CommandSubstitutionDecision`] indicating how to handle the substitution:
    /// - [`commands::CommandSubstitutionDecision::Allow`]: Execute the substitution body normally.
    /// - [`commands::CommandSubstitutionDecision::ReplaceOutput`]: Do not execute; use the
    ///   provided output string as the substitution result.
    /// - [`commands::CommandSubstitutionDecision::Error`]: Abort expansion with an error.
    ///
    /// # Feature Flag
    ///
    /// This method is only available when the `experimental-filters` feature is enabled.
    #[cfg(feature = "experimental-filters")]
    fn pre_command_substitution(
        &self,
        _context: &commands::CommandSubstitutionContext<'_>,
    ) -> commands::CommandSubstitutionDecision {
        commands::CommandSubstitutionDecision::Allow
    }

    /// Clones these extensions for use in a subshell.
    ///
    /// This method is called when the shell creates a subshell (e.g., for `(...)`,
    /// command substitution execution, or pipelines). The cloned extensions are
    /// used in the child shell context.
    ///
    /// # Policy State Preservation
    ///
    /// For extensions implementing policy hooks ([`Self::post_expansion_pre_dispatch_simple_command`],
    /// [`Self::pre_command_substitution`]), the cloned extensions should maintain
    /// consistent policy behavior. This typically means:
    ///
    /// - Sharing configuration (e.g., blocklists, allow lists) via `Arc`
    /// - Sharing counters or statistics via `Arc<AtomicUsize>` or similar
    /// - Sharing mutable state via `Arc<Mutex<T>>` or `Arc<RwLock<T>>`
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use brush_core::extensions::ShellExtensions;
    ///
    /// struct PolicyExtensions {
    ///     // Shared blocklist across parent and child shells
    ///     blocklist: Arc<Vec<String>>,
    ///     // Shared counter for auditing
    ///     command_count: Arc<std::sync::atomic::AtomicUsize>,
    /// }
    ///
    /// impl ShellExtensions for PolicyExtensions {
    ///     fn clone_for_subshell(&self) -> Box<dyn ShellExtensions> {
    ///         Box::new(Self {
    ///             blocklist: Arc::clone(&self.blocklist),
    ///             command_count: Arc::clone(&self.command_count),
    ///         })
    ///     }
    ///
    ///     // ... other methods
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// This method is temporary and will be removed when the trait is migrated to
    /// a generic parameter on `Shell<SE: ShellExtensions>`. At that point, the
    /// standard `Clone` trait will be used instead.
    fn clone_for_subshell(&self) -> Box<dyn ShellExtensions>;
}

/// Default implementation of [`ShellExtensions`] that provides no filtering and
/// supplies default behaviors.
///
/// This is a zero-sized type that incurs no runtime overhead.
#[derive(Debug, Default, Copy, Clone)]
pub struct DefaultExtensions;

impl ShellExtensions for DefaultExtensions {
    fn clone_for_subshell(&self) -> Box<dyn ShellExtensions> {
        Box::new(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_extensions_formats_error() {
        let ext = DefaultExtensions;
        let err = error::Error::from(error::ErrorKind::CommandNotFound("test".to_string()));
        let shell = Shell::default();

        let formatted = ext.format_error(&err, &shell);
        assert!(formatted.starts_with("error:"));
        assert!(formatted.contains("command not found: test"));
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_pre_expand_word_continues() {
        let ext = DefaultExtensions;
        let word = "test";

        let result = ext.pre_expand_word(word);
        assert!(matches!(
            result,
            filter::PreFilterResult::Continue(w) if w == word
        ));
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_post_expand_word_returns_unchanged() {
        use crate::expansion::Expansion;

        let ext = DefaultExtensions;
        let input = Expansion::default();
        let output: Result<Expansion, error::Error> = Ok(input);

        let result = ext.post_expand_word(output);
        assert!(matches!(result, filter::PostFilterResult::Return(Ok(_))));
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_pre_exec_external_command_continues() {
        let ext = DefaultExtensions;
        let mut cmd = std::process::Command::new("echo");
        cmd.arg("test");

        let result = ext.pre_exec_external_command(cmd);
        assert!(matches!(
            result,
            filter::PreFilterResult::Continue(c) if c.get_program() == "echo"
        ));
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_pre_source_script_continues() {
        use std::path::Path;

        let ext = DefaultExtensions;
        let args = shell::ScriptArgs {
            path: Path::new("/test.sh"),
            args: vec![],
        };

        let result = ext.pre_source_script(args);
        assert!(matches!(
            result,
            filter::PreFilterResult::Continue(a) if a.path == Path::new("/test.sh")
        ));
    }

    #[test]
    fn default_extensions_clone_for_subshell_works() {
        let ext = DefaultExtensions;
        let cloned = ext.clone_for_subshell();

        let shell = Shell::default();
        let err = error::Error::from(error::ErrorKind::CommandNotFound("test".to_string()));
        let formatted = cloned.format_error(&err, &shell);
        assert!(formatted.contains("command not found: test"));
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_needs_command_context_returns_false() {
        let ext = DefaultExtensions;
        assert!(!ext.needs_command_context());
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_post_expansion_pre_dispatch_returns_continue() {
        use crate::ExecutionParameters;

        let ext = DefaultExtensions;
        let mut shell = Shell::default();
        let params = ExecutionParameters::default();
        let cmd = commands::SimpleCommand::new(
            commands::ShellForCommand::ParentShell(&mut shell),
            params,
            "echo".to_string(),
            vec![commands::CommandArg::String("echo".to_string())],
        );

        let context = commands::CommandContext {
            executable: "echo",
            arguments: &[],
            raw_command: None,
            pipeline: None,
            redirections: vec![],
            dispatch_target: commands::DispatchTarget::Builtin,
        };

        let result = ext.post_expansion_pre_dispatch_simple_command(&cmd, &context);
        assert!(matches!(result, commands::PolicyDecision::Continue));
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_pre_command_substitution_returns_allow() {
        use std::borrow::Cow;

        let ext = DefaultExtensions;
        let context = commands::CommandSubstitutionContext {
            raw_body: Some(Cow::Borrowed("echo hello")),
            syntax: commands::SubstitutionSyntax::DollarParen,
        };

        let result = ext.pre_command_substitution(&context);
        assert!(matches!(
            result,
            commands::CommandSubstitutionDecision::Allow
        ));
    }

    #[cfg(feature = "experimental-filters")]
    #[test]
    fn default_extensions_pre_command_substitution_handles_backticks() {
        use std::borrow::Cow;

        let ext = DefaultExtensions;
        let context = commands::CommandSubstitutionContext {
            raw_body: Some(Cow::Borrowed("date")),
            syntax: commands::SubstitutionSyntax::Backticks,
        };

        let result = ext.pre_command_substitution(&context);
        assert!(matches!(
            result,
            commands::CommandSubstitutionDecision::Allow
        ));
    }
}
