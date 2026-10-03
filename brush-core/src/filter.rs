//! Hooks for command execution and script sourcing.
//!
//! `CmdExecFilter` and `SourceFilter` are static shell extensions.
//! Default filters pass operations through.

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

/// Pre-hook result.
#[derive(Debug)]
#[non_exhaustive]
pub enum PreFilterResult<I, O> {
    /// Continue with this input.
    Continue(I),
    /// Skip the operation and return this output.
    Return(O),
}

/// Post-hook result.
#[derive(Debug)]
#[non_exhaustive]
pub enum PostFilterResult<O> {
    /// Return this output.
    Return(O),
}

//
// Operation-specific output type aliases
//

/// Output type for simple command execution.
pub type SimpleCmdOutput = Result<ExecutionSpawnResult, error::Error>;

/// Output type for script sourcing.
pub type SourceScriptOutput = Result<ExecutionResult, error::Error>;

//
// Filter parameter types
//

/// Parameters for simple command filtering.
#[non_exhaustive]
pub struct SimpleCmdParams<'a, SE: ShellExtensions> {
    /// Shell executing the command.
    pub shell: &'a Shell<SE>,
    command_name: Cow<'a, str>,
    args: Cow<'a, [String]>,
    raw_args: Option<&'a [CommandArg]>,
}

impl<'a, SE: ShellExtensions> SimpleCmdParams<'a, SE> {
    /// Builds parameters from string arguments.
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

    /// Builds parameters from raw [`CommandArg`] values.
    pub fn from_command_args(
        shell: &'a Shell<SE>,
        command_name: impl Into<Cow<'a, str>>,
        args: &'a [CommandArg],
    ) -> Self {
        Self {
            shell,
            command_name: command_name.into(),
            args: Cow::Borrowed(&[]),
            raw_args: Some(args),
        }
    }

    /// Returns the command name.
    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    /// Returns command arguments, converting raw values on demand.
    pub fn args(&self) -> Cow<'_, [String]> {
        if let Some(raw) = self.raw_args {
            Cow::Owned(raw.iter().map(|a| a.to_string()).collect())
        } else {
            Cow::Borrowed(&self.args)
        }
    }
}

/// Parameters for script sourcing filtering.
#[non_exhaustive]
pub struct SourceScriptParams<'a, SE: ShellExtensions> {
    /// Shell sourcing the script.
    pub shell: &'a Shell<SE>,
    /// Script path.
    pub path: Cow<'a, Path>,
    /// Script arguments.
    pub args: Cow<'a, [String]>,
}

impl<'a, SE: ShellExtensions> SourceScriptParams<'a, SE> {
    /// Builds source parameters.
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

/// Hooks for command execution.
///
/// Simple hooks observe or short-circuit commands. They do not rewrite them.
/// A filter panic escapes the shell.
pub trait CmdExecFilter: Clone + Default + Send + Sync + 'static {
    /// Runs before a simple command.
    ///
    /// `Return` skips the command and post-hook.
    #[allow(unused_variables)]
    fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> impl std::future::Future<Output = PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput>>
    + Send {
        async { PreFilterResult::Continue(params) }
    }

    /// Runs after a simple command.
    #[allow(unused_variables)]
    fn post_simple_cmd(
        &self,
        result: SimpleCmdOutput,
    ) -> impl std::future::Future<Output = PostFilterResult<SimpleCmdOutput>> + Send {
        async { PostFilterResult::Return(result) }
    }
}

/// Hooks for `.` and `source`.
///
/// `Continue` may replace the path and arguments. `Return` skips the source and post-hook.
/// A filter panic escapes the shell.
pub trait SourceFilter: Clone + Default + Send + Sync + 'static {
    /// Runs before sourcing a script.
    #[allow(unused_variables)]
    fn pre_source_script<'a, SE: ShellExtensions>(
        &self,
        params: SourceScriptParams<'a, SE>,
    ) -> impl std::future::Future<
        Output = PreFilterResult<SourceScriptParams<'a, SE>, SourceScriptOutput>,
    > + Send {
        async { PreFilterResult::Continue(params) }
    }

    /// Runs after sourcing a script.
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

/// Default command filter.
#[derive(Clone, Default, Debug)]
pub struct NoOpCmdExecFilter;

impl CmdExecFilter for NoOpCmdExecFilter {}

/// Default source filter.
#[derive(Clone, Default, Debug)]
pub struct NoOpSourceFilter;

impl SourceFilter for NoOpSourceFilter {}

//
// Filter composition
//

/// Two filters composed in sequence.
///
/// Pre-hooks run first, then second. Post-hooks run second, then first.
#[derive(Debug)]
pub struct FilterStack<First, Second> {
    /// First filter.
    pub first: First,
    /// Second filter.
    pub second: Second,
}

impl<First, Second> FilterStack<First, Second> {
    /// Creates a filter stack.
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
        // Stop after a short-circuit.
        let params = match self.first.pre_simple_cmd(params).await {
            PreFilterResult::Continue(p) => p,
            PreFilterResult::Return(r) => return PreFilterResult::Return(r),
        };
        self.second.pre_simple_cmd(params).await
    }

    async fn post_simple_cmd(&self, result: SimpleCmdOutput) -> PostFilterResult<SimpleCmdOutput> {
        let PostFilterResult::Return(result) = self.second.post_simple_cmd(result).await;
        self.first.post_simple_cmd(result).await
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

/// Fluent command-filter composition.
pub trait CmdExecFilterExt: CmdExecFilter + Sized {
    /// Adds a later filter.
    fn and_then<F: CmdExecFilter>(self, next: F) -> FilterStack<Self, F> {
        FilterStack::new(self, next)
    }
}

impl<T: CmdExecFilter> CmdExecFilterExt for T {}

/// Fluent source-filter composition.
pub trait SourceFilterExt: SourceFilter + Sized {
    /// Adds a later filter.
    fn and_then<F: SourceFilter>(self, next: F) -> FilterStack<Self, F> {
        FilterStack::new(self, next)
    }
}

impl<T: SourceFilter> SourceFilterExt for T {}

#[cfg(test)]
#[allow(
    clippy::unused_async_trait_impl,
    clippy::unwrap_used,
    reason = "test hooks are synchronous and test locks are local"
)]
mod tests {
    use super::*;
    use crate::extensions::DefaultShellExtensions;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Recorder {
        pre: &'static str,
        post: &'static str,
        stop: bool,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl CmdExecFilter for Recorder {
        async fn pre_simple_cmd<'a, SE: ShellExtensions>(
            &self,
            params: SimpleCmdParams<'a, SE>,
        ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
            self.events.lock().unwrap().push(self.pre);
            if self.stop {
                return PreFilterResult::Return(Ok(ExecutionSpawnResult::Completed(
                    ExecutionResult::new(1),
                )));
            }
            PreFilterResult::Continue(params)
        }

        async fn post_simple_cmd(
            &self,
            result: SimpleCmdOutput,
        ) -> PostFilterResult<SimpleCmdOutput> {
            self.events.lock().unwrap().push(self.post);
            PostFilterResult::Return(result)
        }
    }

    fn params(
        shell: &Shell<DefaultShellExtensions>,
    ) -> SimpleCmdParams<'_, DefaultShellExtensions> {
        SimpleCmdParams::new(shell, "cmd", &[])
    }

    #[tokio::test]
    async fn filter_stack_reverses_post_hooks() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let filters = FilterStack::new(
            Recorder {
                pre: "a-pre",
                post: "a-post",
                events: events.clone(),
                ..Default::default()
            },
            Recorder {
                pre: "b-pre",
                post: "b-post",
                events: events.clone(),
                ..Default::default()
            },
        );
        let shell = Shell::default();

        assert!(matches!(
            filters.pre_simple_cmd(params(&shell)).await,
            PreFilterResult::Continue(_)
        ));
        let _ = filters
            .post_simple_cmd(Ok(ExecutionSpawnResult::Completed(
                ExecutionResult::success(),
            )))
            .await;

        assert_eq!(
            *events.lock().unwrap(),
            ["a-pre", "b-pre", "b-post", "a-post"]
        );
    }

    #[tokio::test]
    async fn filter_stack_stops_after_short_circuit() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let filters = FilterStack::new(
            Recorder {
                pre: "stop",
                stop: true,
                events: events.clone(),
                ..Default::default()
            },
            Recorder {
                pre: "next",
                events: events.clone(),
                ..Default::default()
            },
        );
        let shell = Shell::default();

        assert!(matches!(
            filters.pre_simple_cmd(params(&shell)).await,
            PreFilterResult::Return(_)
        ));
        assert_eq!(*events.lock().unwrap(), ["stop"]);
    }
}
