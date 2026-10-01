//! Owned descriptor capabilities follow the final command and never replace shell IO.
#![cfg(test)]
#![cfg(unix)]
#![allow(clippy::panic_in_result_fn)]
#![allow(
    clippy::unused_async_trait_impl,
    reason = "Test hooks defer effects until polled"
)]

use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;

use brush_core::extensions::{DefaultErrorFormatter, ShellExtensionsImpl};
use brush_core::filter::{
    CmdExecFilter, DelegatedFd, ExternalCmdOutput, ExternalCmdParams, PreFilterResult,
};
use brush_core::{Error, ErrorKind, Shell, ShellExtensions};

const TARGET: brush_core::ShellFd = 37;
const REPORT: &str = "BRUSH_DESCRIPTOR_REPORT";

#[derive(Clone, Default)]
struct Delegate {
    channel: Option<Arc<UnixStream>>,
    report: PathBuf,
    deny: bool,
}

impl CmdExecFilter for Delegate {
    async fn pre_external_cmd<'a, SE: ShellExtensions>(
        &self,
        mut params: ExternalCmdParams<'a, SE>,
    ) -> PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput> {
        let result = (|| -> Result<(), Error> {
            let channel = self.channel.as_ref().ok_or(ErrorKind::PermissionDenied)?;
            params
                .command
                .delegate_fd(DelegatedFd::new(TARGET, channel.try_clone()?.into())?)?;
            params.command.env(REPORT, &self.report);
            Ok(())
        })();
        match result {
            Ok(()) => PreFilterResult::Continue(params),
            Err(error) => PreFilterResult::Return(Err(error)),
        }
    }

    async fn authorize_external_cmd<SE: ShellExtensions>(
        &self,
        params: &ExternalCmdParams<'_, SE>,
    ) -> Result<(), Error> {
        assert_eq!(params.command.delegated_fds().len(), 1);
        assert_eq!(params.command.delegated_fds()[0].target(), TARGET);
        if self.deny {
            Err(Error::from(ErrorKind::PermissionDenied).into_terminating())
        } else {
            Ok(())
        }
    }
}

#[test]
#[ignore = "self-executable child fixture"]
fn descriptor_child() -> anyhow::Result<()> {
    let report = std::env::var_os(REPORT).ok_or_else(|| anyhow::anyhow!("missing report path"))?;
    let mut bytes = [0u8; 12];
    let mut offset = 0;
    while offset < bytes.len() {
        let remaining = &mut bytes[offset..];
        // SAFETY: the writable slice supplies a valid pointer and exact length.
        // A missing/closed descriptor is a normal read error, not assumed valid.
        let count = unsafe { libc::read(TARGET, remaining.as_mut_ptr().cast(), remaining.len()) };
        if count < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if count == 0 {
            anyhow::bail!("delegated descriptor closed before the payload");
        }
        offset += usize::try_from(count)?;
    }
    std::fs::write(report, bytes)?;
    Ok(())
}

async fn exercise(deny: bool, collision: bool) -> anyhow::Result<()> {
    type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, Delegate>;
    let dir = tempfile::tempdir()?;
    let report = dir.path().join("report");
    let (mut sender, receiver) = UnixStream::pair()?;
    sender.write_all(b"owned-access")?;
    let mut shell = Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(Delegate {
            channel: Some(Arc::new(receiver)),
            report: report.clone(),
            deny,
        })
        .working_dir(dir.path().to_owned())
        .build()
        .await?;
    let executable = std::env::current_exe()?;
    let command = format!(
        "{} --ignored --exact descriptor_child{}",
        brush_core::escape::single_quote(&executable.to_string_lossy()),
        if collision { " 37> collision" } else { "" }
    );
    let params = shell.default_exec_params();
    let result = shell
        .run_string(&command, &brush_core::SourceInfo::default(), &params)
        .await;
    if deny || collision {
        assert!(
            !report.exists(),
            "a refused or colliding delegation must not execute the child"
        );
        assert!(result.is_err() || !result?.is_success());
    } else {
        assert!(result?.is_success());
        assert_eq!(std::fs::read(report)?, b"owned-access");
    }
    Ok(())
}

#[tokio::test]
async fn explicit_descriptor_reaches_only_the_authorized_child() -> anyhow::Result<()> {
    exercise(false, false).await
}

#[tokio::test]
async fn final_denial_prevents_descriptor_child_execution() -> anyhow::Result<()> {
    exercise(true, false).await
}

#[tokio::test]
async fn delegation_cannot_replace_a_shell_redirection() -> anyhow::Result<()> {
    exercise(false, true).await
}

#[test]
fn delegation_rejects_standard_streams_and_duplicate_targets() -> anyhow::Result<()> {
    let (a, b) = UnixStream::pair()?;
    assert!(DelegatedFd::new(1, a.into()).is_err());
    let mut command = brush_core::filter::ExternalCommand::new("unused");
    command.delegate_fd(DelegatedFd::new(TARGET, b.try_clone()?.into())?)?;
    assert!(
        command
            .delegate_fd(DelegatedFd::new(TARGET, b.into())?)
            .is_err()
    );
    Ok(())
}

#[test]
fn delegation_does_not_retain_an_inheritable_ambient_source() -> anyhow::Result<()> {
    use std::os::fd::AsRawFd as _;
    let (source, _peer) = UnixStream::pair()?;
    let inheritable = nix::unistd::dup(&source)?;
    assert_eq!(
        nix::fcntl::fcntl(&inheritable, nix::fcntl::FcntlArg::F_GETFD)?,
        0
    );
    let delegated = DelegatedFd::new(TARGET, inheritable)?;
    let flags = nix::fcntl::fcntl(delegated.source(), nix::fcntl::FcntlArg::F_GETFD)?;
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--ignored", "--exact", "descriptor_absence_child"])
        .env(
            "BRUSH_ABSENT_FD",
            delegated.source().as_raw_fd().to_string(),
        )
        .status()?;
    assert!(
        status.success(),
        "unrelated child inherited the source capability"
    );
    Ok(())
}

#[test]
#[ignore = "self-executable descriptor absence fixture"]
fn descriptor_absence_child() -> anyhow::Result<()> {
    let descriptor: i32 = std::env::var("BRUSH_ABSENT_FD")?.parse()?;
    // SAFETY: querying an integer descriptor has no memory access requirements.
    assert_eq!(unsafe { libc::fcntl(descriptor, libc::F_GETFD) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EBADF)
    );
    Ok(())
}

#[test]
fn delegation_rejects_unrepresentable_mapping_sentinel() -> anyhow::Result<()> {
    let (source, _peer) = UnixStream::pair()?;
    assert!(DelegatedFd::new(i32::MAX, source.into()).is_err());
    Ok(())
}
