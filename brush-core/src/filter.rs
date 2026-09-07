//! Filter infrastructure for intercepting and modifying shell operations.
//!
//! This module provides a zero-cost abstraction for pre/post-operation filtering.
//! Filters can observe, modify, or short-circuit operations like command execution,
//! word expansion, and script sourcing.
//!
//! # Design
//!
//! Filters are defined as traits with async methods that receive operation-specific
//! parameters and return filter results. The default implementations pass through
//! unchanged, enabling zero-cost behavior when no custom filtering is needed.
//!
//! # Example
//!
//! ```ignore
//! use brush_core::filter::{CmdExecFilter, PreFilterResult, PostFilterResult};
//!
//! #[derive(Clone, Default)]
//! struct LoggingFilter;
//!
//! impl CmdExecFilter for LoggingFilter {
//!     async fn pre_simple_cmd<SE: ShellExtensions>(
//!         &self,
//!         params: SimpleCmdParams<'_, SE>,
//!     ) -> PreFilterResult<SimpleCmdParams<'_, SE>, SimpleCmdOutput> {
//!         println!("Executing: {}", params.command_name());
//!         PreFilterResult::Continue(params)
//!     }
//! }
//! ```

use std::borrow::Cow;
use std::path::Path;

use crate::commands::CommandArg;
use crate::error;
use crate::extensions::ShellExtensions;
use crate::results::{ExecutionResult, ExecutionSpawnResult};
use crate::shell::Shell;

//
// Filter result types
//

/// Result of a pre-operation filter.
///
/// Determines whether the operation should proceed (possibly with modified input)
/// or be short-circuited with an immediate result.
#[derive(Debug)]
#[non_exhaustive]
pub enum PreFilterResult<I, O> {
    /// Continue with the operation using the (possibly modified) input.
    Continue(I),
    /// Short-circuit the operation and return this output immediately.
    Return(O),
}

/// Result of a post-operation filter.
///
/// Allows the filter to observe or transform the operation's output.
/// Future versions may add variants for retry semantics.
#[derive(Debug)]
#[non_exhaustive]
pub enum PostFilterResult<O> {
    /// Return this (possibly modified) output.
    Return(O),
}

//
// Operation-specific output type aliases
//

/// Output type for simple command execution.
pub type SimpleCmdOutput = Result<ExecutionSpawnResult, error::Error>;

/// Output type for external command execution.
pub type ExternalCmdOutput = Result<ExecutionSpawnResult, error::Error>;

/// Output type for script sourcing.
pub type SourceScriptOutput = Result<ExecutionResult, error::Error>;

//
// Filter parameter types
//

/// Parameters for simple command execution filtering.
///
/// Provides access to command details and shell state for filtering decisions.
#[non_exhaustive]
pub struct SimpleCmdParams<'a, SE: ShellExtensions> {
    /// The shell executing the command.
    pub shell: &'a Shell<SE>,
    /// The name of the command being executed.
    pub command_name: Cow<'a, str>,
    /// The arguments to the command (including argv[0]).
    ///
    /// This field stores pre-allocated strings. For zero-cost construction when
    /// the filter won't inspect args, use [`SimpleCmdParams::from_command_args`]
    /// which defers string conversion.
    pub args: Cow<'a, [String]>,
    /// Raw command arguments (if constructed via `from_command_args`).
    /// This allows zero-allocation construction when filters don't inspect args.
    raw_args: Option<&'a [CommandArg]>,
}

impl<'a, SE: ShellExtensions> SimpleCmdParams<'a, SE> {
    /// Creates new simple command parameters with pre-allocated string args.
    pub fn new(
        shell: &'a Shell<SE>,
        command_name: impl Into<Cow<'a, str>>,
        args: impl Into<Cow<'a, [String]>>,
    ) -> Self {
        Self {
            shell,
            command_name: command_name.into(),
            args: args.into(),
            raw_args: None,
        }
    }

    /// Creates new simple command parameters from raw [`CommandArg`] slice.
    ///
    /// This constructor is zero-cost for filters that don't inspect arguments.
    /// String conversion is deferred until [`args()`](Self::args) or
    /// [`args_to_strings()`](Self::args_to_strings) is called.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let params = SimpleCmdParams::from_command_args(&shell, "echo", &args);
    /// // No allocation yet - args are stored by reference
    ///
    /// // Allocation happens here, only if needed:
    /// let string_args = params.args_to_strings();
    /// ```
    pub fn from_command_args(
        shell: &'a Shell<SE>,
        command_name: impl Into<Cow<'a, str>>,
        args: &'a [CommandArg],
    ) -> Self {
        Self {
            shell,
            command_name: command_name.into(),
            args: Cow::Borrowed(&[]), // Empty placeholder
            raw_args: Some(args),
        }
    }

    /// Returns the command name.
    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    /// Returns the command arguments as strings.
    ///
    /// If this instance was created via [`from_command_args`](Self::from_command_args),
    /// this returns an empty slice. Use [`args_to_strings`](Self::args_to_strings)
    /// to get the actual arguments with lazy allocation.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Converts command arguments to strings, allocating if necessary.
    ///
    /// If this instance was created via [`new`](Self::new) with pre-allocated args,
    /// returns them directly. If created via [`from_command_args`](Self::from_command_args),
    /// performs the string conversion now.
    ///
    /// For iteration, use `args_to_strings().iter()` on the returned `Cow`.
    pub fn args_to_strings(&self) -> Cow<'_, [String]> {
        if let Some(raw) = self.raw_args {
            Cow::Owned(raw.iter().map(|a| a.to_string()).collect())
        } else {
            Cow::Borrowed(&self.args)
        }
    }
}

/// Parameters for external command execution filtering.
///
/// Provides access to the command being spawned and shell state.
#[non_exhaustive]
pub struct ExternalCmdParams<'a, SE: ShellExtensions> {
    /// The shell executing the command.
    pub shell: &'a Shell<SE>,
    /// The external command builder (introspectable and modifiable).
    pub command: ExternalCommand,
}

impl<'a, SE: ShellExtensions> ExternalCmdParams<'a, SE> {
    /// Creates new external command parameters.
    pub const fn new(shell: &'a Shell<SE>, command: ExternalCommand) -> Self {
        Self { shell, command }
    }
}

/// Parameters for script sourcing filtering.
#[non_exhaustive]
pub struct SourceScriptParams<'a, SE: ShellExtensions> {
    /// The shell sourcing the script.
    pub shell: &'a Shell<SE>,
    /// The path to the script being sourced.
    pub path: Cow<'a, Path>,
    /// The arguments to pass to the script.
    pub args: Cow<'a, [String]>,
}

impl<'a, SE: ShellExtensions> SourceScriptParams<'a, SE> {
    /// Creates new source script parameters.
    pub fn new(
        shell: &'a Shell<SE>,
        path: impl Into<Cow<'a, Path>>,
        args: impl Into<Cow<'a, [String]>>,
    ) -> Self {
        Self {
            shell,
            path: path.into(),
            args: args.into(),
        }
    }

    /// Returns the script path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the script arguments.
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

//
// External command builder (introspectable)
//

/// An introspectable command builder for external process execution.
///
/// Unlike `std::process::Command`, this type allows inspection of all
/// configured state, enabling filters to make decisions based on the
/// full command configuration.
///
/// # Known Limitations
///
/// The following `std::process::Command` features are not yet supported:
/// - stdin/stdout/stderr redirection configuration
/// - Unix-specific options (uid, gid, process group)
/// - Windows-specific options (creation flags)
///
/// These may be added in future versions as the filter system matures.
#[derive(Debug, Clone)]
pub struct ExternalCommand {
    program: std::ffi::OsString,
    args: Vec<std::ffi::OsString>,
    envs: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    current_dir: Option<std::path::PathBuf>,
    env_clear: bool,
}

impl ExternalCommand {
    /// Creates a new external command for the given program.
    pub fn new(program: impl AsRef<std::ffi::OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            envs: Vec::new(),
            current_dir: None,
            env_clear: false,
        }
    }

    /// Returns the program path.
    pub fn program(&self) -> &std::ffi::OsStr {
        &self.program
    }

    /// Returns the arguments.
    pub fn args(&self) -> &[std::ffi::OsString] {
        &self.args
    }

    /// Returns the environment variables that will be set.
    pub fn envs(&self) -> &[(std::ffi::OsString, std::ffi::OsString)] {
        &self.envs
    }

    /// Returns the current directory, if set.
    pub fn current_dir(&self) -> Option<&std::path::Path> {
        self.current_dir.as_deref()
    }

    /// Returns whether the environment will be cleared.
    pub const fn env_clear(&self) -> bool {
        self.env_clear
    }

    /// Sets the program path.
    pub fn set_program(&mut self, program: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        program.as_ref().clone_into(&mut self.program);
        self
    }

    /// Adds an argument.
    pub fn arg(&mut self, arg: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    /// Adds multiple arguments.
    pub fn args_extend<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|s| s.as_ref().to_owned()));
        self
    }

    /// Clears the environment.
    pub const fn clear_env(&mut self) -> &mut Self {
        self.env_clear = true;
        self
    }

    /// Sets an environment variable.
    pub fn env(
        &mut self,
        key: impl AsRef<std::ffi::OsStr>,
        val: impl AsRef<std::ffi::OsStr>,
    ) -> &mut Self {
        self.envs
            .push((key.as_ref().to_owned(), val.as_ref().to_owned()));
        self
    }

    /// Sets the current directory.
    pub fn set_current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
        self.current_dir = Some(dir.as_ref().to_owned());
        self
    }

    /// Converts this into a `std::process::Command`.
    #[must_use]
    pub fn into_std_command(self) -> std::process::Command {
        let mut cmd = std::process::Command::new(&self.program);

        if self.env_clear {
            cmd.env_clear();
        }

        for (key, val) in &self.envs {
            cmd.env(key, val);
        }

        cmd.args(&self.args);

        if let Some(dir) = &self.current_dir {
            cmd.current_dir(dir);
        }

        cmd
    }
}

//
// Filter traits
//

/// Filter for command execution operations.
///
/// Provides hooks for both simple commands (builtins, functions, externals)
/// and external command spawning specifically.
///
/// # Hook Semantics
///
/// - `pre_simple_cmd` / `post_simple_cmd`: Called for ALL commands (builtins, functions,
///   external). Use for observation, modification, or short-circuiting. When using the
///   `$p =>` form of [`with_filter!`], returned params are captured and can be used.
///
/// - `pre_external_cmd` / `post_external_cmd`: Called only for external process spawning.
///   The [`ExternalCommand`] in params can be modified and changes will be applied.
///
/// # Panic Safety
///
/// **Filter implementations must not panic.** Panics in filter methods will propagate
/// through the shell and may terminate the process. There is no `catch_unwind` wrapper
/// around filter invocations for performance reasons. If your filter interacts with
/// fallible operations (locks, I/O, etc.), handle errors gracefully rather than
/// panicking.
pub trait CmdExecFilter: Clone + Default + Send + Sync + 'static {
    /// Called before a simple command is executed.
    ///
    /// Can inspect and modify the command parameters, or short-circuit execution by returning
    /// `PreFilterResult::Return`. This hook is called for ALL command types
    /// (builtins, shell functions, external commands).
    ///
    /// When returning `Continue(params)` with the `$p =>` form of [`with_filter!`], the
    /// returned params are captured and used. For external command modification specifically,
    /// prefer `pre_external_cmd` which provides the introspectable [`ExternalCommand`] builder.
    #[allow(unused_variables)]
    fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> impl std::future::Future<Output = PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput>>
    + Send {
        async { PreFilterResult::Continue(params) }
    }

    /// Called after a simple command is executed.
    ///
    /// Can inspect/modify the execution result.
    #[allow(unused_variables)]
    fn post_simple_cmd(
        &self,
        result: SimpleCmdOutput,
    ) -> impl std::future::Future<Output = PostFilterResult<SimpleCmdOutput>> + Send {
        async { PostFilterResult::Return(result) }
    }

    /// Called before an external command is spawned.
    ///
    /// Can inspect and **modify** the command configuration, or short-circuit
    /// with an error. Unlike `pre_simple_cmd`, modifications to `ExternalCommand`
    /// in the returned `Continue` params ARE applied to the spawned process.
    #[allow(unused_variables)]
    fn pre_external_cmd<'a, SE: ShellExtensions>(
        &self,
        params: ExternalCmdParams<'a, SE>,
    ) -> impl std::future::Future<
        Output = PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput>,
    > + Send {
        async { PreFilterResult::Continue(params) }
    }

    /// Called after an external command is spawned.
    ///
    /// Can inspect/modify the spawn result.
    #[allow(unused_variables)]
    fn post_external_cmd(
        &self,
        result: ExternalCmdOutput,
    ) -> impl std::future::Future<Output = PostFilterResult<ExternalCmdOutput>> + Send {
        async { PostFilterResult::Return(result) }
    }
}

/// Filter for script sourcing operations.
///
/// Provides hooks for the `.` and `source` builtins that execute scripts
/// in the current shell context.
///
/// # Panic Safety
///
/// **Filter implementations must not panic.** Panics in filter methods will propagate
/// through the shell and may terminate the process. There is no `catch_unwind` wrapper
/// around filter invocations for performance reasons.
pub trait SourceFilter: Clone + Default + Send + Sync + 'static {
    /// Called before a script is sourced.
    ///
    /// Can inspect/modify the source parameters or short-circuit.
    #[allow(unused_variables)]
    fn pre_source_script<'a, SE: ShellExtensions>(
        &self,
        params: SourceScriptParams<'a, SE>,
    ) -> impl std::future::Future<
        Output = PreFilterResult<SourceScriptParams<'a, SE>, SourceScriptOutput>,
    > + Send {
        async { PreFilterResult::Continue(params) }
    }

    /// Called after a script is sourced.
    ///
    /// Can inspect/modify the source result.
    #[allow(unused_variables)]
    fn post_source_script(
        &self,
        result: SourceScriptOutput,
    ) -> impl std::future::Future<Output = PostFilterResult<SourceScriptOutput>> + Send {
        async { PostFilterResult::Return(result) }
    }
}

//
// No-op filter implementations
//

/// No-op command execution filter.
///
/// Passes through all operations unchanged. This is the default filter
/// and incurs zero runtime overhead due to monomorphization.
#[derive(Clone, Default, Debug)]
pub struct NoOpCmdExecFilter;

impl CmdExecFilter for NoOpCmdExecFilter {}

/// No-op source filter.
///
/// Passes through all operations unchanged.
#[derive(Clone, Default, Debug)]
pub struct NoOpSourceFilter;

impl SourceFilter for NoOpSourceFilter {}

//
// Filter composition
//

/// Two filters composed in sequence.
///
/// When executing:
/// - Pre-filters run in order: `first`, then `second`
/// - Post-filters run in reverse order: `second`, then `first`
///
/// This follows standard middleware/interceptor semantics where the
/// "outer" filter (first) wraps the "inner" filter (second).
///
/// # Example
///
/// ```ignore
/// use brush_core::filter::{FilterStack, CmdExecFilterExt};
///
/// // Using FilterStack::new directly:
/// let filter = FilterStack::new(LoggingFilter, SecurityFilter);
///
/// // Using the extension trait (equivalent):
/// let filter = LoggingFilter.and_then(SecurityFilter);
///
/// // Execution order for a command:
/// // 1. LoggingFilter::pre_simple_cmd (logs "starting")
/// // 2. SecurityFilter::pre_simple_cmd (checks permissions)
/// // 3. <actual command executes>
/// // 4. SecurityFilter::post_simple_cmd (checks output)
/// // 5. LoggingFilter::post_simple_cmd (logs "completed")
/// ```
#[derive(Debug)]
pub struct FilterStack<First, Second> {
    /// The first (outer) filter in the stack.
    pub first: First,
    /// The second (inner) filter in the stack.
    pub second: Second,
}

impl<First, Second> FilterStack<First, Second> {
    /// Creates a new filter stack with the given filters.
    ///
    /// Pre-filters run `first` then `second`; post-filters run `second` then `first`.
    pub const fn new(first: First, second: Second) -> Self {
        Self { first, second }
    }
}

impl<First: Clone, Second: Clone> Clone for FilterStack<First, Second> {
    fn clone(&self) -> Self {
        Self {
            first: self.first.clone(),
            second: self.second.clone(),
        }
    }
}

impl<First: Default, Second: Default> Default for FilterStack<First, Second> {
    fn default() -> Self {
        Self {
            first: First::default(),
            second: Second::default(),
        }
    }
}

impl<First: CmdExecFilter, Second: CmdExecFilter> CmdExecFilter for FilterStack<First, Second> {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        // Run first filter, short-circuit if it returns
        let params = match self.first.pre_simple_cmd(params).await {
            PreFilterResult::Continue(p) => p,
            PreFilterResult::Return(r) => return PreFilterResult::Return(r),
        };
        // Run second filter
        self.second.pre_simple_cmd(params).await
    }

    async fn post_simple_cmd(&self, result: SimpleCmdOutput) -> PostFilterResult<SimpleCmdOutput> {
        // Post-filters run in reverse order: second then first
        let PostFilterResult::Return(result) = self.second.post_simple_cmd(result).await;
        self.first.post_simple_cmd(result).await
    }

    async fn pre_external_cmd<'a, SE: ShellExtensions>(
        &self,
        params: ExternalCmdParams<'a, SE>,
    ) -> PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput> {
        let params = match self.first.pre_external_cmd(params).await {
            PreFilterResult::Continue(p) => p,
            PreFilterResult::Return(r) => return PreFilterResult::Return(r),
        };
        self.second.pre_external_cmd(params).await
    }

    async fn post_external_cmd(
        &self,
        result: ExternalCmdOutput,
    ) -> PostFilterResult<ExternalCmdOutput> {
        let PostFilterResult::Return(result) = self.second.post_external_cmd(result).await;
        self.first.post_external_cmd(result).await
    }
}

impl<First: SourceFilter, Second: SourceFilter> SourceFilter for FilterStack<First, Second> {
    async fn pre_source_script<'a, SE: ShellExtensions>(
        &self,
        params: SourceScriptParams<'a, SE>,
    ) -> PreFilterResult<SourceScriptParams<'a, SE>, SourceScriptOutput> {
        let params = match self.first.pre_source_script(params).await {
            PreFilterResult::Continue(p) => p,
            PreFilterResult::Return(r) => return PreFilterResult::Return(r),
        };
        self.second.pre_source_script(params).await
    }

    async fn post_source_script(
        &self,
        result: SourceScriptOutput,
    ) -> PostFilterResult<SourceScriptOutput> {
        let PostFilterResult::Return(result) = self.second.post_source_script(result).await;
        self.first.post_source_script(result).await
    }
}

/// Extension trait for fluent command execution filter composition.
///
/// This trait is automatically implemented for all types that implement
/// [`CmdExecFilter`], providing the [`and_then`](CmdExecFilterExt::and_then) method.
pub trait CmdExecFilterExt: CmdExecFilter + Sized {
    /// Composes this filter with another, creating a [`FilterStack`].
    ///
    /// The resulting filter runs `self`'s pre-filter first, then `next`'s.
    /// Post-filters run in reverse order.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let filter = AuditingFilter::new()
    ///     .and_then(RateLimitFilter::new(5))
    ///     .and_then(SecurityFilter::new());
    /// ```
    fn and_then<F: CmdExecFilter>(self, next: F) -> FilterStack<Self, F> {
        FilterStack::new(self, next)
    }
}

impl<T: CmdExecFilter> CmdExecFilterExt for T {}

/// Extension trait for fluent source filter composition.
///
/// This trait is automatically implemented for all types that implement
/// [`SourceFilter`], providing the [`and_then`](SourceFilterExt::and_then) method.
pub trait SourceFilterExt: SourceFilter + Sized {
    /// Composes this filter with another, creating a [`FilterStack`].
    ///
    /// The resulting filter runs `self`'s pre-filter first, then `next`'s.
    /// Post-filters run in reverse order.
    fn and_then<F: SourceFilter>(self, next: F) -> FilterStack<Self, F> {
        FilterStack::new(self, next)
    }
}

impl<T: SourceFilter> SourceFilterExt for T {}

//
// Filter invocation macro
//

/// Macro for invoking a filter around an operation.
///
/// Handles the boilerplate of calling pre/post filter methods and
/// matching on the results. When used with no-op filters, the compiler
/// optimizes away all filter overhead.
///
/// # Variants
///
/// - Basic form: `with_filter!(shell, filter, pre, post, params, body)` - ignores returned params
/// - With params: `with_filter!(shell, filter, pre, post, params, |p| body)` - captures returned params as `p`
///
/// # Arguments
///
/// * `$shell` - The shell instance
/// * `$filter_accessor` - Method to get the filter from the shell
/// * `$pre_method` - The pre-operation filter method name
/// * `$post_method` - The post-operation filter method name
/// * `$params` - Expression that constructs the filter parameters
/// * `$body` - The operation body to execute if the filter allows
///
/// # Example
///
/// ```ignore
/// // Basic form (returned params not captured):
/// with_filter!(
///     shell,
///     cmd_exec_filter,
///     pre_simple_cmd,
///     post_simple_cmd,
///     SimpleCmdParams::new(&shell, cmd_name, &args),
///     execute_impl().await
/// )
///
/// // With params capture (for hooks that modify):
/// with_filter!(
///     shell,
///     source_filter,
///     pre_source_script,
///     post_source_script,
///     SourceScriptParams::new(&shell, path, &args),
///     p => run_script(p.path(), p.args()).await
/// )
///
/// // With params capture and finally block (runs after post-filter):
/// with_filter!(
///     shell,
///     cmd_exec_filter,
///     pre_external_cmd,
///     post_external_cmd,
///     params,
///     p => execute_command(p).await,
///     finally { cleanup_after() }
/// )
/// ```
#[macro_export]
macro_rules! with_filter {
    // Basic form: ignores returned params from Continue
    ($shell:expr, $filter_accessor:ident, $pre_method:ident, $post_method:ident, $params:expr, $body:expr) => {{
        let __filter = $shell.$filter_accessor().clone();
        match __filter.$pre_method($params).await {
            $crate::filter::PreFilterResult::Continue(_p) => {
                let __result = $body;
                let $crate::filter::PostFilterResult::Return(__r) =
                    __filter.$post_method(__result).await;
                __r
            }
            $crate::filter::PreFilterResult::Return(__r) => __r,
        }
    }};
    // Form with params capture: "$p =>" syntax binds returned params to $p for use in $body
    ($shell:expr, $filter_accessor:ident, $pre_method:ident, $post_method:ident, $params:expr, $p:ident => $body:expr) => {{
        let __filter = $shell.$filter_accessor().clone();
        match __filter.$pre_method($params).await {
            $crate::filter::PreFilterResult::Continue($p) => {
                let __result = $body;
                let $crate::filter::PostFilterResult::Return(__r) =
                    __filter.$post_method(__result).await;
                __r
            }
            $crate::filter::PreFilterResult::Return(__r) => __r,
        }
    }};
    // Form with params capture AND finally block: runs $finally after post-filter completes
    ($shell:expr, $filter_accessor:ident, $pre_method:ident, $post_method:ident, $params:expr, $p:ident => $body:expr, finally $finally:block) => {{
        let __filter = $shell.$filter_accessor().clone();
        match __filter.$pre_method($params).await {
            $crate::filter::PreFilterResult::Continue($p) => {
                let __result = $body;
                let $crate::filter::PostFilterResult::Return(__r) =
                    __filter.$post_method(__result).await;
                $finally
                __r
            }
            $crate::filter::PreFilterResult::Return(__r) => __r,
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shell;
    use crate::extensions::DefaultShellExtensions;

    /// Test that the basic form of `with_filter!` compiles and works correctly
    /// with the no-op filter (zero-cost case).
    #[tokio::test]
    async fn test_with_filter_basic_form_noop() {
        let shell: Shell<DefaultShellExtensions> = Shell::default();

        // Construct params separately to help type inference
        let args = ["echo".to_string(), "hello".to_string()];
        let params = SimpleCmdParams::new(&shell, "echo", args.as_slice());

        let result: SimpleCmdOutput = with_filter!(
            shell,
            cmd_exec_filter,
            pre_simple_cmd,
            post_simple_cmd,
            params,
            Ok(crate::ExecutionSpawnResult::Completed(
                crate::ExecutionResult::success()
            ))
        );

        assert!(result.is_ok());
    }

    /// Test that the params-capture form of `with_filter!` compiles and works correctly.
    /// Note: This test uses the basic form since the `|p|` form requires context where SE is known.
    /// The `|p|` form is tested implicitly via production code usage.
    #[tokio::test]
    async fn test_with_filter_params_capture_form_noop() {
        let shell: Shell<DefaultShellExtensions> = Shell::default();

        // Construct params separately to help type inference
        let params = ExternalCmdParams::new(&shell, ExternalCommand::new("/bin/echo"));

        // Use basic form (ignoring params) since |p| form requires more context for inference
        let result: ExternalCmdOutput = with_filter!(
            shell,
            cmd_exec_filter,
            pre_external_cmd,
            post_external_cmd,
            params,
            Ok(crate::ExecutionSpawnResult::Completed(
                crate::ExecutionResult::success(),
            ))
        );

        assert!(result.is_ok());
    }

    /// Test `with_filter!` for source script operations.
    #[tokio::test]
    async fn test_with_filter_source_script_noop() {
        let shell: Shell<DefaultShellExtensions> = Shell::default();

        // Construct params separately to help type inference
        let empty_args: [String; 0] = [];
        let params =
            SourceScriptParams::new(&shell, std::path::Path::new("/tmp/test.sh"), &empty_args);

        // Use basic form (ignoring params) since |p| form requires more context for inference
        let result: SourceScriptOutput = with_filter!(
            shell,
            source_filter,
            pre_source_script,
            post_source_script,
            params,
            Ok(crate::ExecutionResult::success())
        );

        assert!(result.is_ok());
    }

    /// Custom filter that tracks calls for testing.
    #[derive(Clone, Default)]
    struct TrackingCmdExecFilter {
        pre_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
        post_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl CmdExecFilter for TrackingCmdExecFilter {
        async fn pre_simple_cmd<'a, SE: crate::extensions::ShellExtensions>(
            &self,
            params: SimpleCmdParams<'a, SE>,
        ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
            self.pre_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            PreFilterResult::Continue(params)
        }

        async fn post_simple_cmd(
            &self,
            result: SimpleCmdOutput,
        ) -> PostFilterResult<SimpleCmdOutput> {
            self.post_called
                .store(true, std::sync::atomic::Ordering::SeqCst);
            PostFilterResult::Return(result)
        }
    }

    /// Custom extensions type for testing with `TrackingCmdExecFilter`.
    #[derive(Clone, Default)]
    #[allow(dead_code)] // filter field used indirectly through ShellExtensions
    struct TrackingExtensions {
        filter: TrackingCmdExecFilter,
    }

    impl crate::extensions::ShellExtensions for TrackingExtensions {
        type ErrorFormatter = crate::extensions::DefaultErrorFormatter;
        type CmdExecFilter = TrackingCmdExecFilter;
        type SourceFilter = NoOpSourceFilter;
    }

    /// Test that a custom filter's pre/post methods are actually called.
    #[tokio::test]
    async fn test_with_filter_custom_filter_called() {
        let pre_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let post_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let filter = TrackingCmdExecFilter {
            pre_called: pre_called.clone(),
            post_called: post_called.clone(),
        };

        let shell: Shell<TrackingExtensions> = Shell::builder_with_extensions()
            .cmd_exec_filter(filter)
            .build()
            .await
            .unwrap();

        // Construct params separately to help type inference
        let args = ["test".to_string()];
        let params = SimpleCmdParams::new(&shell, "test", args.as_slice());

        let _result: SimpleCmdOutput = with_filter!(
            shell,
            cmd_exec_filter,
            pre_simple_cmd,
            post_simple_cmd,
            params,
            Ok(crate::ExecutionSpawnResult::Completed(
                crate::ExecutionResult::success()
            ))
        );

        assert!(
            pre_called.load(std::sync::atomic::Ordering::SeqCst),
            "pre_simple_cmd should have been called"
        );
        assert!(
            post_called.load(std::sync::atomic::Ordering::SeqCst),
            "post_simple_cmd should have been called"
        );
    }

    /// Test that returning `PreFilterResult::Return` short-circuits execution.
    #[derive(Clone, Default)]
    struct ShortCircuitFilter;

    impl CmdExecFilter for ShortCircuitFilter {
        async fn pre_simple_cmd<'a, SE: crate::extensions::ShellExtensions>(
            &self,
            _params: SimpleCmdParams<'a, SE>,
        ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
            PreFilterResult::Return(Ok(crate::ExecutionSpawnResult::Completed(
                crate::ExecutionResult::new(42),
            )))
        }
    }

    /// Custom extensions type for testing with `ShortCircuitFilter`.
    #[derive(Clone, Default)]
    struct ShortCircuitExtensions;

    impl crate::extensions::ShellExtensions for ShortCircuitExtensions {
        type ErrorFormatter = crate::extensions::DefaultErrorFormatter;
        type CmdExecFilter = ShortCircuitFilter;
        type SourceFilter = NoOpSourceFilter;
    }

    /// Test short-circuit behavior.
    #[tokio::test]
    #[allow(clippy::panic_in_result_fn)]
    async fn test_with_filter_short_circuit() -> Result<(), crate::Error> {
        let shell: Shell<ShortCircuitExtensions> = Shell::builder_with_extensions()
            .cmd_exec_filter(ShortCircuitFilter)
            .build()
            .await?;

        let body_executed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let body_executed_clone = body_executed.clone();

        // Construct params separately to help type inference
        let args = ["test".to_string()];
        let params = SimpleCmdParams::new(&shell, "test", args.as_slice());

        let result: SimpleCmdOutput = with_filter!(
            shell,
            cmd_exec_filter,
            pre_simple_cmd,
            post_simple_cmd,
            params,
            {
                body_executed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(crate::ExecutionSpawnResult::Completed(
                    crate::ExecutionResult::success(),
                ))
            }
        );

        assert!(
            !body_executed.load(std::sync::atomic::Ordering::SeqCst),
            "body should NOT have been executed due to short-circuit"
        );

        // Verify we got the short-circuit result
        let result = result?;
        let crate::ExecutionSpawnResult::Completed(exec_result) = result else {
            unreachable!("expected Completed variant")
        };
        assert_eq!(u8::from(exec_result.exit_code), 42);
        Ok(())
    }

    // ========================================================================
    // FilterStack composition tests
    // ========================================================================

    /// A filter that records when its pre/post methods are called.
    #[derive(Clone, Default)]
    struct OrderTrackingFilter {
        id: &'static str,
        pre_order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
        post_order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl OrderTrackingFilter {
        fn new(
            id: &'static str,
            pre_order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
            post_order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
        ) -> Self {
            Self {
                id,
                pre_order,
                post_order,
            }
        }
    }

    impl CmdExecFilter for OrderTrackingFilter {
        async fn pre_simple_cmd<'a, SE: crate::extensions::ShellExtensions>(
            &self,
            params: SimpleCmdParams<'a, SE>,
        ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
            if let Ok(mut order) = self.pre_order.lock() {
                order.push(self.id);
            }
            PreFilterResult::Continue(params)
        }

        async fn post_simple_cmd(
            &self,
            result: SimpleCmdOutput,
        ) -> PostFilterResult<SimpleCmdOutput> {
            if let Ok(mut order) = self.post_order.lock() {
                order.push(self.id);
            }
            PostFilterResult::Return(result)
        }
    }

    /// Test that `FilterStack` runs pre-filters in order (first then second)
    /// and post-filters in reverse order (second then first).
    #[tokio::test]
    #[allow(clippy::significant_drop_tightening)]
    async fn test_filter_stack_ordering() {
        let pre_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let post_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let filter_a = OrderTrackingFilter::new("A", pre_order.clone(), post_order.clone());
        let filter_b = OrderTrackingFilter::new("B", pre_order.clone(), post_order.clone());

        // Compose: A.and_then(B)
        let composed = filter_a.and_then(filter_b);

        // Create a mock shell for testing
        let shell: Shell<DefaultShellExtensions> = Shell::default();
        let args = ["test".to_string()];
        let params = SimpleCmdParams::new(&shell, "test", args.as_slice());

        // Run pre-filter
        let result = composed.pre_simple_cmd(params).await;
        assert!(matches!(result, PreFilterResult::Continue(_)));

        // Run post-filter
        let result = Ok(crate::ExecutionSpawnResult::Completed(
            crate::ExecutionResult::success(),
        ));
        let _ = composed.post_simple_cmd(result).await;

        // Verify ordering
        let pre = pre_order.lock().unwrap();
        let post = post_order.lock().unwrap();

        assert_eq!(*pre, vec!["A", "B"], "pre-filters should run first→second");
        assert_eq!(
            *post,
            vec!["B", "A"],
            "post-filters should run second→first (reverse)"
        );
    }

    /// A filter that short-circuits in `pre_simple_cmd`.
    #[derive(Clone, Default)]
    struct ShortCircuitAtPreFilter {
        pre_order: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl CmdExecFilter for ShortCircuitAtPreFilter {
        async fn pre_simple_cmd<'a, SE: crate::extensions::ShellExtensions>(
            &self,
            _params: SimpleCmdParams<'a, SE>,
        ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
            if let Ok(mut order) = self.pre_order.lock() {
                order.push("SHORT");
            }
            PreFilterResult::Return(Ok(crate::ExecutionSpawnResult::Completed(
                crate::ExecutionResult::new(99),
            )))
        }
    }

    /// Test that short-circuiting in the first filter prevents the second filter from running.
    #[tokio::test]
    #[allow(clippy::significant_drop_tightening)]
    async fn test_filter_stack_short_circuit_first() {
        let pre_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let short_circuit = ShortCircuitAtPreFilter {
            pre_order: pre_order.clone(),
        };
        let post_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let tracking = OrderTrackingFilter::new("B", pre_order.clone(), post_order);

        // Short-circuit filter is first
        let composed = FilterStack::new(short_circuit, tracking);

        let shell: Shell<DefaultShellExtensions> = Shell::default();
        let args = ["test".to_string()];
        let params = SimpleCmdParams::new(&shell, "test", args.as_slice());

        let result = composed.pre_simple_cmd(params).await;

        // Should have short-circuited
        assert!(matches!(result, PreFilterResult::Return(_)));

        // Only the short-circuit filter should have run
        let pre = pre_order.lock().unwrap();
        assert_eq!(
            *pre,
            vec!["SHORT"],
            "second filter should not run when first short-circuits"
        );
    }

    /// Test that `and_then` extension trait works correctly.
    #[tokio::test]
    async fn test_cmd_exec_filter_ext_and_then() {
        let filter_a = NoOpCmdExecFilter;
        let filter_b = NoOpCmdExecFilter;

        // This should compile and create a FilterStack
        let _composed: FilterStack<NoOpCmdExecFilter, NoOpCmdExecFilter> =
            filter_a.and_then(filter_b);
    }

    /// Test that three filters can be composed.
    #[tokio::test]
    #[allow(clippy::significant_drop_tightening)]
    async fn test_filter_stack_triple_composition() {
        let pre_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let post_order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let filter_a = OrderTrackingFilter::new("A", pre_order.clone(), post_order.clone());
        let filter_b = OrderTrackingFilter::new("B", pre_order.clone(), post_order.clone());
        let filter_c = OrderTrackingFilter::new("C", pre_order.clone(), post_order.clone());

        // Compose: A.and_then(B).and_then(C)
        let composed = filter_a.and_then(filter_b).and_then(filter_c);

        let shell: Shell<DefaultShellExtensions> = Shell::default();
        let args = ["test".to_string()];
        let params = SimpleCmdParams::new(&shell, "test", args.as_slice());

        let PreFilterResult::Continue(_) = composed.pre_simple_cmd(params).await else {
            unreachable!("expected Continue")
        };

        let result = Ok(crate::ExecutionSpawnResult::Completed(
            crate::ExecutionResult::success(),
        ));
        let _ = composed.post_simple_cmd(result).await;

        let pre = pre_order.lock().unwrap();
        let post = post_order.lock().unwrap();

        assert_eq!(*pre, vec!["A", "B", "C"], "pre-filters: A → B → C");
        assert_eq!(*post, vec!["C", "B", "A"], "post-filters: C → B → A");
    }

    // ========================================================================
    // Error propagation tests
    // ========================================================================

    /// A filter that returns an error result in `pre_simple_cmd`.
    #[derive(Clone, Default)]
    struct ErrorReturningFilter;

    impl CmdExecFilter for ErrorReturningFilter {
        async fn pre_simple_cmd<'a, SE: crate::extensions::ShellExtensions>(
            &self,
            _params: SimpleCmdParams<'a, SE>,
        ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
            PreFilterResult::Return(Err(crate::error::Error::from(
                crate::error::ErrorKind::CommandNotFound("blocked by filter".to_string()),
            )))
        }
    }

    /// Custom extensions type for testing with `ErrorReturningFilter`.
    #[derive(Clone, Default)]
    struct ErrorExtensions;

    impl crate::extensions::ShellExtensions for ErrorExtensions {
        type ErrorFormatter = crate::extensions::DefaultErrorFormatter;
        type CmdExecFilter = ErrorReturningFilter;
        type SourceFilter = NoOpSourceFilter;
    }

    /// Test that errors returned from filters propagate correctly.
    #[tokio::test]
    #[allow(clippy::panic_in_result_fn, clippy::panic)]
    async fn test_filter_error_propagation() -> Result<(), crate::Error> {
        let shell: Shell<ErrorExtensions> = Shell::builder_with_extensions()
            .cmd_exec_filter(ErrorReturningFilter)
            .build()
            .await?;

        let args = ["test".to_string()];
        let params = SimpleCmdParams::new(&shell, "test", args.as_slice());

        let result: SimpleCmdOutput = with_filter!(
            shell,
            cmd_exec_filter,
            pre_simple_cmd,
            post_simple_cmd,
            params,
            Ok(crate::ExecutionSpawnResult::Completed(
                crate::ExecutionResult::success()
            ))
        );

        // Should have received the error from the filter
        let Err(err) = result else {
            panic!("expected error result from filter");
        };
        assert!(
            matches!(err.kind(), crate::error::ErrorKind::CommandNotFound(_)),
            "expected CommandNotFound error, got: {err:?}"
        );
        Ok(())
    }

    /// A filter that uses Arc<Mutex<_>> for state, demonstrating safe shared state patterns.
    #[derive(Clone, Default)]
    struct StatefulFilter {
        call_count: std::sync::Arc<std::sync::Mutex<u32>>,
    }

    impl CmdExecFilter for StatefulFilter {
        async fn pre_simple_cmd<'a, SE: crate::extensions::ShellExtensions>(
            &self,
            params: SimpleCmdParams<'a, SE>,
        ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
            // Demonstrate safe mutex handling - don't unwrap, handle poisoning gracefully
            if let Ok(mut count) = self.call_count.lock() {
                *count += 1;
            }
            PreFilterResult::Continue(params)
        }
    }

    /// Custom extensions type for testing with `StatefulFilter`.
    #[derive(Clone, Default)]
    struct StatefulExtensions;

    impl crate::extensions::ShellExtensions for StatefulExtensions {
        type ErrorFormatter = crate::extensions::DefaultErrorFormatter;
        type CmdExecFilter = StatefulFilter;
        type SourceFilter = NoOpSourceFilter;
    }

    /// Test that stateful filters with Arc<Mutex> work correctly and
    /// demonstrate proper error handling patterns.
    #[tokio::test]
    #[allow(clippy::significant_drop_tightening)]
    async fn test_stateful_filter_with_shared_state() {
        let call_count = std::sync::Arc::new(std::sync::Mutex::new(0u32));
        let filter = StatefulFilter {
            call_count: call_count.clone(),
        };

        let shell: Shell<StatefulExtensions> = Shell::builder_with_extensions()
            .cmd_exec_filter(filter)
            .build()
            .await
            .unwrap();

        // Run filter twice
        for _ in 0..2 {
            let args = ["test".to_string()];
            let params = SimpleCmdParams::new(&shell, "test", args.as_slice());
            let _result: SimpleCmdOutput = with_filter!(
                shell,
                cmd_exec_filter,
                pre_simple_cmd,
                post_simple_cmd,
                params,
                Ok(crate::ExecutionSpawnResult::Completed(
                    crate::ExecutionResult::success()
                ))
            );
        }

        // Verify state was updated
        let count = call_count.lock().unwrap();
        assert_eq!(*count, 2, "filter should have been called twice");
    }

    /// Test that a filter gracefully handles a poisoned mutex (doesn't panic).
    /// This demonstrates the recommended error handling pattern for filter authors.
    #[tokio::test]
    #[allow(clippy::panic_in_result_fn, clippy::panic)]
    async fn test_filter_handles_poisoned_mutex_gracefully() -> Result<(), crate::Error> {
        let call_count = std::sync::Arc::new(std::sync::Mutex::new(0u32));

        // Poison the mutex by panicking while holding the lock in another thread
        let count_clone = call_count.clone();
        let handle = std::thread::spawn(move || {
            let _guard = count_clone.lock().unwrap();
            panic!("intentional panic to poison mutex");
        });
        // Wait for the thread to finish (it will panic)
        let _ = handle.join();

        // Verify the mutex is poisoned
        assert!(
            call_count.lock().is_err(),
            "mutex should be poisoned after panic"
        );

        // Now test that our filter pattern handles this gracefully
        let filter = StatefulFilter {
            call_count: call_count.clone(),
        };

        let shell: Shell<StatefulExtensions> = Shell::builder_with_extensions()
            .cmd_exec_filter(filter)
            .build()
            .await?;

        let args = ["test".to_string()];
        let params = SimpleCmdParams::new(&shell, "test", args.as_slice());

        // This should NOT panic even though the mutex is poisoned,
        // because our filter uses `if let Ok(...)` pattern
        let result: SimpleCmdOutput = with_filter!(
            shell,
            cmd_exec_filter,
            pre_simple_cmd,
            post_simple_cmd,
            params,
            Ok(crate::ExecutionSpawnResult::Completed(
                crate::ExecutionResult::success()
            ))
        );

        // Operation should still succeed (filter gracefully ignores poisoned mutex)
        assert!(result.is_ok());
        Ok(())
    }
}
