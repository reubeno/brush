//! Tests that reading input that isn't there yet doesn't hold up the shell's other tasks.
//!
//! On a runtime with a single thread, a read that blocked the thread would keep the task
//! writing what it waits for from ever running. These run the shell on one.

#![cfg(test)]
#![cfg(unix)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};

/// How long a script may take before it's taken to hang.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Runs `script` and then `check` in a new shell on a runtime with a single thread,
/// returning `check`'s exit status. The runtime gets a thread of its own: once a read
/// blocks it, nothing on it can notice the time running out, so the test waits from
/// outside.
fn run_then_check(script: &'static str, check: &'static str) -> Result<u8> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(anyhow::Error::from)
            .and_then(|runtime| runtime.block_on(run_in_new_shell(script, check)));
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(TIMEOUT) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(anyhow::anyhow!("the script hung")),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(anyhow::anyhow!("the shell's thread panicked"))
        }
    }
}

async fn run_in_new_shell(script: &str, check: &str) -> Result<u8> {
    let mut shell = brush_core::Shell::builder()
        .default_builtins(BuiltinSet::BashMode)
        .build()
        .await?;
    let params = shell.default_exec_params();
    let source_info = brush_core::SourceInfo::default();

    let _ = shell.run_string(script, &source_info, &params).await?;
    let result = shell.run_string(check, &source_info, &params).await?;
    Ok(u8::from(result.exit_code))
}

#[test]
fn read_waits_for_a_process_substitution_in_a_command_substitution() -> Result<()> {
    let status = run_then_check(
        "f() { local n=0; while read -r l; do n=$((n + 1)); done < <(for i in 1 2 3; do echo $i; done); echo $n; }; x=$(f)",
        "[[ $x == 3 ]]",
    )?;
    assert_eq!(status, 0);
    Ok(())
}

#[test]
fn read_waits_for_a_line_written_in_pieces() -> Result<()> {
    let status = run_then_check(
        "x=$(read -r y < <(echo -n hel; echo lo); echo \"$y\")",
        "[[ $x == hello ]]",
    )?;
    assert_eq!(status, 0);
    Ok(())
}

#[test]
fn mapfile_waits_for_a_process_substitution_in_a_command_substitution() -> Result<()> {
    let status = run_then_check(
        "x=$(mapfile -t a < <(echo a; echo b); echo \"${#a[@]}:${a[1]}\")",
        "[[ $x == 2:b ]]",
    )?;
    assert_eq!(status, 0);
    Ok(())
}
