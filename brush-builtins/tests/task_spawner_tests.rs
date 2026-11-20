//! Integration tests for starting the shell's tasks through a host's `TaskSpawner`.

#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::Result;
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};
use brush_core::{
    ErrorKind,
    extensions::{
        DefaultErrorFormatter, DefaultProcessLauncher, DefaultTaskSpawner, ShellExtensionsImpl,
        TaskSpawner,
    },
    tasks::TaskHandle,
};

/// Counts the tasks it's asked to start, and starts them as the default
/// spawner does.
#[derive(Clone, Default)]
struct CountingSpawner {
    spawned: Arc<AtomicUsize>,
    spawned_blocking: Arc<AtomicUsize>,
}

impl TaskSpawner for CountingSpawner {
    fn spawn<F>(&self, future: F) -> TaskHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawned.fetch_add(1, Ordering::Relaxed);
        DefaultTaskSpawner.spawn(future)
    }

    fn spawn_blocking<F>(
        &self,
        future: F,
    ) -> impl Future<Output = TaskHandle<F::Output>> + Send + use<F>
    where
        F: Future + Send + 'static,
        F::Output: Send + Sync + 'static,
    {
        self.spawned_blocking.fetch_add(1, Ordering::Relaxed);
        DefaultTaskSpawner.spawn_blocking(future)
    }
}

/// Starts tasks with [`tokio::task::spawn_local`], so they run on the
/// [`tokio::task::LocalSet`] the caller drives, as a host whose thread isn't
/// driven by tokio would start them.
#[derive(Clone, Default)]
struct LocalSpawner;

impl TaskSpawner for LocalSpawner {
    fn spawn<F>(&self, future: F) -> TaskHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let handle = tokio::task::spawn_local(future);
        TaskHandle::from_future(async move {
            handle
                .await
                .map_err(|err| ErrorKind::TaskError(Box::new(err)).into())
        })
    }

    #[allow(
        clippy::async_yields_async,
        reason = "the future yields the finished task's handle"
    )]
    fn spawn_blocking<F>(
        &self,
        future: F,
    ) -> impl Future<Output = TaskHandle<F::Output>> + Send + use<F>
    where
        F: Future + Send + 'static,
        F::Output: Send + Sync + 'static,
    {
        // Only the caller's thread can run the task, so it can't help but hold
        // the caller up: hand back its handle once it has finished.
        let task = self.spawn(future);
        async move {
            let result = task.await;
            TaskHandle::from_future(std::future::ready(result))
        }
    }
}

/// Runs `script` in a shell that starts its tasks with `spawner`, returning the
/// script's exit status.
async fn run<TS: TaskSpawner>(spawner: TS, script: &str) -> Result<u8> {
    let mut shell = brush_core::Shell::builder_with_extensions::<
        ShellExtensionsImpl<DefaultErrorFormatter, DefaultProcessLauncher, TS>,
    >()
    .default_builtins(BuiltinSet::BashMode)
    .task_spawner(spawner)
    .build()
    .await?;

    let params = shell.default_exec_params();
    let _ = shell
        .run_string(script, &brush_core::SourceInfo::default(), &params)
        .await?;

    Ok(shell.last_exit_status())
}

/// A script whose pipeline stages, asynchronous list, and command substitutions
/// all run as tasks, and which succeeds only if each of them produced its output.
const SCRIPT: &str = r#"
    piped=$(echo hi | { read -r line; echo "${line}!"; })
    background=$(echo bg & wait)
    [[ ${piped} == "hi!" && ${background} == "bg" ]]
"#;

#[tokio::test]
async fn the_shells_tasks_are_started_by_the_hosts_spawner() -> Result<()> {
    let spawner = CountingSpawner::default();

    assert_eq!(run(spawner.clone(), SCRIPT).await?, 0);

    assert!(spawner.spawned.load(Ordering::Relaxed) > 0);
    assert!(spawner.spawned_blocking.load(Ordering::Relaxed) > 0);

    Ok(())
}

#[tokio::test]
async fn the_shell_runs_with_tasks_on_a_local_set() -> Result<()> {
    let status = tokio::task::LocalSet::new()
        .run_until(run(LocalSpawner, SCRIPT))
        .await?;

    assert_eq!(status, 0);

    Ok(())
}
