//! Shell extensions support.

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
}
