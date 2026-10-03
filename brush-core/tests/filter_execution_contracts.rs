//! Runtime contracts for static filters.
#![cfg(test)]
#![allow(
    clippy::panic_in_result_fn,
    clippy::unused_async_trait_impl,
    clippy::unwrap_used,
    reason = "test hooks are synchronous and test locks are local"
)]

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use brush_core::{
    ExecutionResult, ExecutionSpawnResult, Shell, SourceInfo,
    extensions::{DefaultErrorFormatter, ShellExtensions, ShellExtensionsImpl},
    filter::{
        CmdExecFilter, NoOpCmdExecFilter, NoOpSourceFilter, PostFilterResult, PreFilterResult,
        SimpleCmdOutput, SimpleCmdParams, SourceFilter, SourceScriptOutput, SourceScriptParams,
    },
};

#[derive(Clone, Default)]
struct Deny(Arc<AtomicBool>);

impl CmdExecFilter for Deny {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        _params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        self.0.store(true, Ordering::SeqCst);
        PreFilterResult::Return(Ok(ExecutionSpawnResult::Completed(ExecutionResult::new(1))))
    }
}

type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, Deny, NoOpSourceFilter>;

#[derive(Clone, Default)]
struct Observe {
    args: Arc<Mutex<Vec<String>>>,
    post_called: Arc<AtomicBool>,
}

impl CmdExecFilter for Observe {
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        *self.args.lock().unwrap() = params.args().into_owned();
        PreFilterResult::Continue(params)
    }

    async fn post_simple_cmd(&self, result: SimpleCmdOutput) -> PostFilterResult<SimpleCmdOutput> {
        self.post_called.store(true, Ordering::SeqCst);
        PostFilterResult::Return(result)
    }
}

type ObserveExtensions = ShellExtensionsImpl<DefaultErrorFormatter, Observe, NoOpSourceFilter>;

#[derive(Clone, Default)]
struct RedirectSource {
    path: Arc<PathBuf>,
    post_called: Arc<AtomicBool>,
}

impl SourceFilter for RedirectSource {
    async fn pre_source_script<'a, SE: ShellExtensions>(
        &self,
        mut params: SourceScriptParams<'a, SE>,
    ) -> PreFilterResult<SourceScriptParams<'a, SE>, SourceScriptOutput> {
        params.path = Cow::Owned((*self.path).clone());
        PreFilterResult::Continue(params)
    }

    async fn post_source_script(
        &self,
        result: SourceScriptOutput,
    ) -> PostFilterResult<SourceScriptOutput> {
        self.post_called.store(true, Ordering::SeqCst);
        PostFilterResult::Return(result)
    }
}

type SourceExtensions =
    ShellExtensionsImpl<DefaultErrorFormatter, NoOpCmdExecFilter, RedirectSource>;

#[tokio::test]
async fn simple_filter_restores_command_environment() -> Result<(), brush_core::Error> {
    let deny = Deny::default();
    let mut shell = Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(deny.clone())
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .build()
        .await?;
    let params = shell.default_exec_params();

    let _ = shell
        .run_string("FILTER_TEMP=one blocked", &SourceInfo::default(), &params)
        .await?;

    assert!(deny.0.load(Ordering::SeqCst), "pre-hook did not run");
    assert_eq!(shell.env_str("FILTER_TEMP"), None);
    Ok(())
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn command_filter_observes_argv() -> Result<(), brush_core::Error> {
    let observe = Observe::default();
    let mut shell = Shell::builder_with_extensions::<ObserveExtensions>()
        .cmd_exec_filter(observe.clone())
        .build()
        .await?;
    let params = shell.default_exec_params();

    #[cfg(unix)]
    let (command, expected) = ("/usr/bin/true first", vec!["/usr/bin/true", "first"]);
    #[cfg(windows)]
    let (command, expected) = ("cmd /c exit 0", vec!["cmd", "/c", "exit", "0"]);

    let result = shell
        .run_string(command, &SourceInfo::default(), &params)
        .await?;

    assert!(result.is_success());
    assert_eq!(*observe.args.lock().unwrap(), expected);
    assert!(observe.post_called.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn source_filter_rewrites_path() -> Result<(), brush_core::Error> {
    let dir = tempfile::tempdir()?;
    let script = dir.path().join("set-value.sh");
    std::fs::write(&script, "FILTER_SOURCE=ok\n")?;

    let redirect = RedirectSource {
        path: Arc::new(script),
        ..Default::default()
    };
    let mut shell = Shell::builder_with_extensions::<SourceExtensions>()
        .source_filter(redirect.clone())
        .build()
        .await?;
    let params = shell.default_exec_params();

    let _ = shell
        .source_script(Path::new("ignored"), std::iter::empty::<String>(), &params)
        .await?;

    assert_eq!(shell.env_str("FILTER_SOURCE").as_deref(), Some("ok"));
    assert!(redirect.post_called.load(Ordering::SeqCst));
    Ok(())
}
