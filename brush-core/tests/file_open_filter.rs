//! File policy observes real shell opens before any read or write effect.
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use brush_core::extensions::{DefaultErrorFormatter, ShellExtensions, ShellExtensionsImpl};
use brush_core::filter::{
    FileOpenAccess, FileOpenFilter, FileOpenParams, FilterStack, NoOpCmdExecFilter,
    NoOpSourceFilter,
};
use brush_core::{Error, ErrorKind, ProfileLoadBehavior, RcLoadBehavior, Shell};

#[derive(Clone, Default)]
struct RecordingPolicy {
    deny: bool,
    seen: Arc<Mutex<Vec<(PathBuf, FileOpenAccess)>>>,
}

impl FileOpenFilter for RecordingPolicy {
    fn pre_open_file<SE: ShellExtensions>(
        &self,
        params: FileOpenParams<'_, SE>,
    ) -> Result<(), Error> {
        self.seen
            .lock()
            .map_err(|_| {
                Error::from(ErrorKind::InternalError(
                    "test policy audit lock poisoned".into(),
                ))
                .into_terminating()
            })?
            .push((params.path.into(), params.access));
        if self.deny {
            Err(Error::from(ErrorKind::PermissionDenied).into_terminating())
        } else {
            Ok(())
        }
    }
}

type Extensions = ShellExtensionsImpl<
    DefaultErrorFormatter,
    NoOpCmdExecFilter,
    NoOpSourceFilter,
    RecordingPolicy,
>;

async fn shell(
    root: &std::path::Path,
    policy: RecordingPolicy,
) -> Result<Shell<Extensions>, Error> {
    Shell::builder_with_extensions::<Extensions>()
        .file_open_filter(policy)
        .working_dir(root.to_owned())
        .profile(ProfileLoadBehavior::Skip)
        .rc(RcLoadBehavior::Skip)
        .build()
        .await
}

#[tokio::test]
async fn file_open_filter_denies_before_creating_or_truncating() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let existing = dir.path().join("existing");
    std::fs::write(&existing, "keep me")?;
    let policy = RecordingPolicy {
        deny: true,
        ..Default::default()
    };
    let mut shell = shell(dir.path(), policy.clone()).await?;
    for script in [
        "> existing",
        "> created",
        ">> existing",
        "<> created",
        "&> existing",
    ] {
        let params = shell.default_exec_params();
        let result = shell
            .run_string(script, &brush_core::SourceInfo::default(), &params)
            .await;
        assert!(result.is_err(), "open must be refused: {script}");
        assert_eq!(std::fs::read_to_string(&existing)?, "keep me");
        assert!(!dir.path().join("created").exists());
    }
    let seen = policy.seen.lock().unwrap();
    assert_eq!(seen.len(), 5);
    assert_eq!(seen[0], (existing, FileOpenAccess::Write));
    assert_eq!(seen[3].1, FileOpenAccess::ReadWrite);
    Ok(())
}

#[tokio::test]
async fn file_open_filter_sees_reads_and_special_fd_paths() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("input"), "data")?;
    let policy = RecordingPolicy {
        deny: true,
        ..Default::default()
    };
    let mut shell = shell(dir.path(), policy.clone()).await?;
    for script in ["< input", "< /dev/null", "< /dev/stdin", "> /dev/fd/1"] {
        let params = shell.default_exec_params();
        let result = shell
            .run_string(script, &brush_core::SourceInfo::default(), &params)
            .await;
        assert!(
            result.is_err(),
            "special and fd paths must not bypass policy: {script}"
        );
    }
    let seen = policy.seen.lock().unwrap();
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[0], (dir.path().join("input"), FileOpenAccess::Read));
    assert_eq!(
        seen[3],
        (shell.absolute_path("/dev/fd/1"), FileOpenAccess::Write)
    );
    Ok(())
}

#[tokio::test]
async fn file_open_filter_allow_path_performs_the_real_open() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let policy = RecordingPolicy::default();
    let mut shell = shell(dir.path(), policy.clone()).await?;
    let params = shell.default_exec_params();
    shell
        .run_string("> created", &brush_core::SourceInfo::default(), &params)
        .await?;
    assert!(dir.path().join("created").is_file());
    assert_eq!(policy.seen.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn file_open_filter_allows_portable_null_and_fd_aliases() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let policy = RecordingPolicy::default();
    let mut shell = shell(dir.path(), policy.clone()).await?;
    // No command reads or writes the streams: these redirects only exercise
    // the real open/alias path without waiting on the host's standard input.
    for script in ["< /dev/null", "< /dev/stdin", "< /dev/fd/0", "> /dev/fd/1"] {
        let params = shell.default_exec_params();
        let result = shell
            .run_string(script, &brush_core::SourceInfo::default(), &params)
            .await?;
        assert_eq!(
            u8::from(result.exit_code),
            0,
            "allowed alias failed: {script}"
        );
    }
    let seen = policy.seen.lock().unwrap();
    assert_eq!(seen.len(), 4, "special opens still consult policy");
    assert_eq!(seen[0].1, FileOpenAccess::Read);
    assert_eq!(seen[3].1, FileOpenAccess::Write);
    Ok(())
}

#[tokio::test]
async fn file_open_filter_denies_source_before_reading_or_running_it() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let script = dir.path().join("script");
    std::fs::write(&script, "> effect")?;
    let policy = RecordingPolicy {
        deny: true,
        ..Default::default()
    };
    let mut shell = shell(dir.path(), policy.clone()).await?;
    let params = shell.default_exec_params();
    let result = shell
        .source_script(&script, std::iter::empty::<String>(), &params)
        .await;
    assert!(result.is_err());
    assert!(!dir.path().join("effect").exists());
    assert_eq!(
        *policy.seen.lock().unwrap(),
        vec![(script, FileOpenAccess::Read)]
    );
    Ok(())
}

#[tokio::test]
async fn file_open_filter_stack_stops_at_the_first_refusal() -> anyhow::Result<()> {
    type Stacked = ShellExtensionsImpl<
        DefaultErrorFormatter,
        NoOpCmdExecFilter,
        NoOpSourceFilter,
        FilterStack<RecordingPolicy, RecordingPolicy>,
    >;
    let dir = tempfile::tempdir()?;
    let first = RecordingPolicy {
        deny: true,
        ..Default::default()
    };
    let second = RecordingPolicy::default();
    let mut shell = Shell::builder_with_extensions::<Stacked>()
        .file_open_filter(FilterStack::new(first.clone(), second.clone()))
        .working_dir(dir.path().to_owned())
        .profile(ProfileLoadBehavior::Skip)
        .rc(RcLoadBehavior::Skip)
        .build()
        .await?;
    let params = shell.default_exec_params();
    assert!(
        shell
            .run_string("> refused", &brush_core::SourceInfo::default(), &params)
            .await
            .is_err()
    );
    assert!(!dir.path().join("refused").exists());
    assert_eq!(first.seen.lock().unwrap().len(), 1);
    assert!(second.seen.lock().unwrap().is_empty());
    Ok(())
}
