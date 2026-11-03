//! Tests that the shell's own reads and writes on pipes never hold up the tasks on
//! the other end.
//!
//! A builtin writing to a full pipe, or reading an empty one, mustn't block a thread
//! the runtime needs: the task at the other end of the pipe may be waiting on that
//! very thread, and then neither ever finishes. The runtimes here have one thread
//! to run tasks on, so a blocked thread hangs the shell every time rather than
//! some of the time.

#![cfg(test)]
#![cfg(unix)]

use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};

/// How long a script may take before it's taken to have hung: less than the test
/// runner gives a test, so a hang is reported as one.
const DEADLINE: Duration = Duration::from_secs(10);

/// Sets `s` to twice what a pipe holds by default: 2 MiB. Pipes hold 16 pages,
/// which is 1 MiB on a kernel with 64 KiB pages.
macro_rules! large {
    () => {
        "s=x; for i in {1..21}; do s=$s$s; done\n"
    };
}

#[derive(Clone, Copy, Debug)]
enum Flavor {
    /// A runtime that runs every task on the thread that drives it.
    CurrentThread,
    /// A runtime with a single worker thread.
    OneWorker,
}

/// Runs `script` in a new shell on a runtime of the given flavor and returns the
/// value it leaves in `result`, or fails if it doesn't finish within [`DEADLINE`].
fn run(flavor: Flavor, script: &'static str) -> Result<String> {
    let (sender, receiver) = mpsc::channel();

    // The shell runs on a thread of its own: a hung runtime can't time itself out.
    std::thread::spawn(move || {
        let outcome = (|| -> Result<String> {
            let mut builder = match flavor {
                Flavor::CurrentThread => tokio::runtime::Builder::new_current_thread(),
                Flavor::OneWorker => {
                    let mut builder = tokio::runtime::Builder::new_multi_thread();
                    builder.worker_threads(1);
                    builder
                }
            };
            builder.enable_all().build()?.block_on(async move {
                let mut shell = brush_core::Shell::builder()
                    .default_builtins(BuiltinSet::BashMode)
                    .build()
                    .await?;
                let params = shell.default_exec_params();
                let status = shell
                    .run_string(script, &brush_core::SourceInfo::default(), &params)
                    .await?;
                if !status.is_success() {
                    bail!(
                        "the script failed with status {}",
                        u8::from(status.exit_code)
                    );
                }
                let result = shell.env_str("result").context("no result")?;
                Ok(result.into_owned())
            })
        })();
        let _ = sender.send(outcome);
    });

    match receiver.recv_timeout(DEADLINE) {
        Ok(outcome) => outcome,
        Err(_) => bail!("{flavor:?}: the script hung:\n{script}"),
    }
}

/// Runs `script` on a runtime of the given flavor, expecting `expected` in `result`.
fn expect(flavor: Flavor, script: &'static str, expected: &str) -> Result<()> {
    let result = run(flavor, script)?;
    if result != expected {
        bail!("{flavor:?}: result is {result:?}, not {expected:?}");
    }
    Ok(())
}

/// Defines a test of `script` for each flavor of runtime.
///
/// On a runtime with a single thread, a builtin's read or write that would block
/// blocks the shell: builtins read and write synchronously, so there's no yielding
/// to the task at the other end until they do so asynchronously.
macro_rules! on_each_runtime {
    ($name:ident, $script:expr, $expected:expr) => {
        mod $name {
            use super::*;

            #[test]
            fn with_one_worker() -> Result<()> {
                expect(Flavor::OneWorker, $script, $expected)
            }

            #[test]
            #[ignore = "builtins read and write synchronously, so they can't yield on a single thread"]
            fn on_the_current_thread() -> Result<()> {
                expect(Flavor::CurrentThread, $script, $expected)
            }
        }
    };
}

// The report in #1576: a builtin's large output after a nested substitution.
on_each_runtime!(
    large_builtin_output_after_a_nested_command_substitution,
    concat!(large!(), "v=$(echo \"$(true)$s\"); result=${#v}"),
    "2097152"
);

on_each_runtime!(
    large_builtin_output_in_a_command_substitution,
    concat!(large!(), "v=$(printf '%s' \"$s\"); result=${#v}"),
    "2097152"
);

on_each_runtime!(
    large_builtin_output_through_a_pipeline_into_a_builtin,
    concat!(
        large!(),
        "shopt -s lastpipe; set +m\n",
        "printf '%s\\n' \"$s\" | read -r v; result=${#v}"
    ),
    "2097152"
);

// Input: `read` waits on a substitution whose writer is another of the shell's tasks.
on_each_runtime!(
    read_from_a_process_substitution_inside_a_command_substitution,
    "result=$(read -r x < <(echo hi); echo \"$x\")",
    "hi"
);

// `read -t` waits for input that a process substitution, another of the shell's
// tasks, is about to write.
on_each_runtime!(
    read_with_a_timeout_from_a_process_substitution_inside_a_command_substitution,
    "result=$(read -t 5 -r x < <(echo hi); echo \"$?:$x\")",
    "0:hi"
);

// Opening a FIFO waits for its other end, here a background job of the same shell.
on_each_runtime!(
    a_fifo_between_a_background_job_and_a_command_substitution,
    concat!(
        "d=$(mktemp -d); mkfifo \"$d/p\"\n",
        "result=$(echo hi > \"$d/p\" & read -r x < \"$d/p\"; echo \"$x\")\n",
        "rm -r \"$d\""
    ),
    "hi"
);

on_each_runtime!(
    large_builtin_output_through_a_fifo_inside_a_command_substitution,
    concat!(
        large!(),
        "d=$(mktemp -d); mkfifo \"$d/p\"\n",
        "result=$(printf '%s\\n' \"$s\" > \"$d/p\" & mapfile -t a < \"$d/p\"; echo \"${#a[0]}\")\n",
        "rm -r \"$d\""
    ),
    "2097152"
);

on_each_runtime!(
    a_fifo_inside_a_background_job,
    concat!(
        "d=$(mktemp -d); mkfifo \"$d/p\"\n",
        "f() { echo hi > \"$d/p\" & read -r x < \"$d/p\"; echo \"$x\" > \"$d/out\"; }\n",
        "f & wait; read -r result < \"$d/out\"; rm -r \"$d\""
    ),
    "hi"
);
