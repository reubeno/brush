//! Experimental shell extensions support.

use std::path::Path;

use crate::{ExecutionResult, commands, error, expansion, filter};

/// Input for the source script operation.
#[derive(Clone, Debug)]
pub struct ScriptArgs<'a> {
    /// The path to the script to source.
    pub path: &'a Path,
    /// The arguments to pass to the script as positional parameters.
    pub args: Vec<&'a str>,
}

/// Marker type for source script filtering.
///
/// This type defines the input/output signature for filtering script
/// sourcing operations.
pub struct SourceScriptOp<'a> {
    marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> crate::filter::FilterableOp for SourceScriptOp<'a> {
    type Input = ScriptArgs<'a>;
    type Output = Result<ExecutionResult, error::Error>;
}

/// Trait for extending shell behavior with custom filters and hooks.
///
/// This trait allows clients to intercept and modify shell operations at
/// key points during execution. All methods have default implementations
/// that perform no filtering.
///
/// Implementations should use interior mutability (`Arc<Mutex<T>>`, etc.) to share
/// mutable state across shell clones, since `clone_for_subshell()` should create
/// a new instance that shares the same underlying state.
///
/// This is an experimental API that may change or be replaced with a
/// generic-based approach in the future.
pub trait ShellExtensions: Send + Sync {
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
    fn pre_source_script<'a>(
        &self,
        input: ScriptArgs<'a>,
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
    fn post_source_script<'a>(
        &self,
        output: <SourceScriptOp<'a> as filter::FilterableOp>::Output,
    ) -> filter::PostFilterResult<SourceScriptOp<'a>> {
        filter::PostFilterResult::Return(output)
    }

    /// Clones these extensions for use in a subshell.
    ///
    /// Implementations should typically share state across clones using `Arc` or
    /// similar mechanisms. This allows subshells to share the same extension behavior
    /// and state as their parent shell.
    ///
    /// # Note
    ///
    /// This method is temporary and will be removed when the trait is migrated to
    /// a generic parameter on `Shell<SE: ShellExtensions>`. At that point, the
    /// standard `Clone` trait will be used instead.
    fn clone_for_subshell(&self) -> Box<dyn ShellExtensions>;
}

/// Default implementation of [`ShellExtensions`] that provides no filtering.
///
/// This is a zero-sized type that incurs no runtime overhead.
#[derive(Debug, Default, Copy, Clone)]
pub struct DefaultExtensions;

impl ShellExtensions for DefaultExtensions {
    fn clone_for_subshell(&self) -> Box<dyn ShellExtensions> {
        Box::new(*self)
    }
}
