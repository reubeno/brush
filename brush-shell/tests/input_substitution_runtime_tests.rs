//! Input-only substitutions must not block a single-threaded executor.

#![cfg(any(unix, windows))]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};
use brush_core::extensions::{
    DefaultErrorFormatter, DefaultProcessLauncher, DefaultTaskSpawner, ShellExtensionsImpl,
};

// Make the producer run before the caller can poll its output reader.
#[derive(Clone, Default)]
struct ProducerFirstSpawner(Arc<tokio::sync::Notify>);

impl brush_core::extensions::TaskSpawner for ProducerFirstSpawner {
    fn spawn<F>(&self, future: F) -> brush_core::tasks::TaskHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let started = self.0.clone();
        DefaultTaskSpawner.spawn(async move {
            started.notify_one();
            future.await
        })
    }

    #[expect(
        clippy::async_yields_async,
        reason = "the spawner contract returns a task handle separately from its result"
    )]
    fn spawn_blocking<F>(
        &self,
        future: F,
    ) -> impl Future<Output = brush_core::tasks::TaskHandle<F::Output>> + Send + use<F>
    where
        F: Future + Send + 'static,
        F::Output: Send + Sync + 'static,
    {
        let started = self.0.clone();
        async move {
            let handle = DefaultTaskSpawner.spawn_blocking(future).await;
            started.notified().await;
            handle
        }
    }
}

#[test]
fn large_input_substitution_on_current_thread() -> Result<()> {
    const CHILD: &str = "BRUSH_INPUT_SUBSTITUTION_RUNTIME_TEST_CHILD";

    if std::env::var_os(CHILD).is_some() {
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                type Extensions = ShellExtensionsImpl<
                    DefaultErrorFormatter,
                    DefaultProcessLauncher,
                    ProducerFirstSpawner,
                >;
                let mut shell = brush_core::Shell::builder_with_extensions::<Extensions>()
                    .default_builtins(BuiltinSet::BashMode)
                    .build()
                    .await?;
                let params = shell.default_exec_params();
                let result = shell
                    .run_string(
                        "x=$(< <(printf %0524288d 0)); [[ ${#x} -eq 524288 ]]",
                        &brush_core::SourceInfo::default(),
                        &params,
                    )
                    .await?;
                ensure!(u8::from(result.exit_code) == 0, "unexpected shell result");
                Ok(())
            });
    }

    // A runtime timeout cannot fire if the executor itself is blocked.
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", "large_input_substitution_on_current_thread"])
        .env(CHILD, "1")
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "child test failed: {status}");
            return Ok(());
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            anyhow::bail!("current-thread substitution timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
async fn wait_for(path: &std::path::Path) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_input_substitution_stops_reading() -> Result<()> {
    let head = Command::new("head")
        .args(["-c", "1", "/dev/zero"])
        .output()?;
    ensure!(
        head.status.success() && head.stdout == [0],
        "head must be able to read /dev/zero"
    );

    let dir = tempfile::tempdir()?;
    let mut shell = brush_core::Shell::builder()
        .default_builtins(BuiltinSet::BashMode)
        .build()
        .await?;
    shell.set_working_dir(dir.path())?;
    let params = shell.default_exec_params();
    let source = brush_core::SourceInfo::default();
    let mut run = Box::pin(shell.run_string(
        "x=$(< <(echo ready > ready; \
         while [[ ! -e go ]]; do sleep 0.01; done; \
         head -c 16777216 /dev/zero 2>/dev/null && echo completed > completed; \
         echo stopped > stopped))",
        &source,
        &params,
    ));

    let ready = dir.path().join("ready");
    tokio::select! {
        _ = &mut run => anyhow::bail!("substitution finished before cancellation"),
        result = wait_for(&ready) => result?,
    }
    drop(run);
    std::fs::write(dir.path().join("go"), "")?;
    wait_for(&dir.path().join("stopped")).await?;
    ensure!(
        !dir.path().join("completed").exists(),
        "producer completed after its substitution was cancelled"
    );
    Ok(())
}
