//! Native process effects ground command correlation and rejected-registration cleanup.
#![cfg(test)]
#![cfg(unix)]
#![allow(clippy::panic_in_result_fn)]
#![allow(
    clippy::unused_async_trait_impl,
    reason = "Test hooks follow the async API"
)]

use std::sync::{Arc, Mutex};

use brush_core::extensions::{DefaultErrorFormatter, ShellExtensionsImpl};
use brush_core::filter::{CmdExecFilter, ExternalCommand};
use brush_core::{Error, ErrorKind, Shell};

type RegisteredCalls = Arc<Mutex<Vec<(Vec<std::ffi::OsString>, u32)>>>;

#[derive(Clone, Default)]
struct Register {
    calls: RegisteredCalls,
    reject: bool,
    pending: bool,
    started: Arc<tokio::sync::Notify>,
}

impl CmdExecFilter for Register {
    async fn external_cmd_spawned(
        &self,
        command: &ExternalCommand,
        pid: Option<u32>,
    ) -> Result<(), Error> {
        let pid = pid.ok_or(ErrorKind::PermissionDenied)?;
        self.calls
            .lock()
            .unwrap()
            .push((command.args().to_vec(), pid));
        self.started.notify_one();
        if self.pending {
            std::future::pending::<()>().await;
        }
        if self.reject {
            Err(ErrorKind::PermissionDenied.into())
        } else {
            Ok(())
        }
    }
}

async fn cancel_spawned_command(pending: bool, kill_on_drop: bool) -> anyhow::Result<i32> {
    type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, Register>;
    let filter = Register {
        pending,
        ..Register::default()
    };
    let mut shell = Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(filter.clone())
        .kill_external_commands_on_drop(kill_on_drop)
        .build()
        .await?;
    let params = shell.default_exec_params();
    let source = brush_core::SourceInfo::default();
    {
        let command = shell.run_string("/bin/sleep 30", &source, &params);
        tokio::pin!(command);
        tokio::select! {
            () = filter.started.notified() => {},
            result = &mut command => {
                result?;
                anyhow::bail!("sleep completed before its registration was observed");
            }
        }
    }
    let calls = filter.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    Ok(i32::try_from(calls[0].1)?)
}

fn child_exists(pid: i32) -> bool {
    // SAFETY: signal zero only queries the fixture's child process.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn cleanup_child(pid: i32) {
    // SAFETY: this PID belongs to the fixture's still-running child.
    let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
    // SAFETY: a null status pointer is allowed; waiting owns no Rust memory.
    let _ = unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
}

async fn assert_child_stops(pid: i32) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while child_exists(pid) && std::time::Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    let survived = child_exists(pid);
    if survived {
        cleanup_child(pid);
    }
    assert!(!survived, "cancelled shell leaked child {pid}");
}

#[tokio::test]
async fn cancelled_pending_registration_stops_child_with_default_shell_lifecycle()
-> anyhow::Result<()> {
    let pid = cancel_spawned_command(true, false).await?;
    assert_child_stops(pid).await;
    Ok(())
}

#[tokio::test]
async fn acknowledged_registration_preserves_default_child_survival() -> anyhow::Result<()> {
    let pid = cancel_spawned_command(false, false).await?;
    let survived = child_exists(pid);
    if survived {
        cleanup_child(pid);
    }
    assert!(survived, "registered child lost ordinary shell lifecycle");
    Ok(())
}

#[tokio::test]
async fn acknowledged_registration_preserves_configured_child_cleanup() -> anyhow::Result<()> {
    let pid = cancel_spawned_command(false, true).await?;
    assert_child_stops(pid).await;
    Ok(())
}

#[tokio::test]
async fn spawned_process_is_correlated_with_its_exact_command() -> anyhow::Result<()> {
    type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, Register>;
    let filter = Register::default();
    let mut shell = Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(filter.clone())
        .build()
        .await?;
    let params = shell.default_exec_params();
    let result = shell
        .run_string(
            "/bin/echo first | /bin/cat",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;
    assert!(result.is_success());
    let calls = filter.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].1, calls[1].1);
    assert!(
        calls
            .iter()
            .any(|(args, _)| args == &[std::ffi::OsString::from("first")])
    );
    assert!(calls.iter().any(|(args, _)| args.is_empty()));
    drop(calls);
    Ok(())
}

#[tokio::test]
async fn rejected_registration_reaps_child_and_stops_shell() -> anyhow::Result<()> {
    type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, Register>;
    let filter = Register {
        reject: true,
        ..Register::default()
    };
    let dir = tempfile::tempdir()?;
    let mut shell = Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(filter.clone())
        .working_dir(dir.path().to_owned())
        .build()
        .await?;
    let params = shell.default_exec_params();
    let result = shell
        .run_string(
            "/bin/sleep 1; echo escaped > marker",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await;
    assert!(result.is_err_and(|error| error.is_terminating()));
    assert!(!dir.path().join("marker").exists());
    let calls = filter.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let pid = i32::try_from(calls[0].1)?;
    drop(calls);
    // SAFETY: signal zero only queries process existence; it reads no memory.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    Ok(())
}
