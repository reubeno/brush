//! Serializing a shell must not replace installed policy with default authority.
#![cfg(test)]
#![cfg(feature = "serde")]
#![allow(clippy::panic_in_result_fn)]

use brush_core::extensions::{DefaultErrorFormatter, ShellExtensions, ShellExtensionsImpl};
use brush_core::filter::{CmdExecFilter, FileOpenFilter, FileOpenParams, SourceFilter};
use brush_core::{ProfileLoadBehavior, RcLoadBehavior, Shell};

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct StatefulPolicy {
    deny: bool,
}
impl CmdExecFilter for StatefulPolicy {}
impl SourceFilter for StatefulPolicy {}
impl FileOpenFilter for StatefulPolicy {
    fn pre_open_file<SE: ShellExtensions>(
        &self,
        _: FileOpenParams<'_, SE>,
    ) -> Result<(), brush_core::Error> {
        if self.deny {
            Err(brush_core::Error::from(brush_core::ErrorKind::PermissionDenied).into_terminating())
        } else {
            Ok(())
        }
    }
}
type Extensions =
    ShellExtensionsImpl<DefaultErrorFormatter, StatefulPolicy, StatefulPolicy, StatefulPolicy>;

#[tokio::test]
async fn policy_serialization_preserves_stateful_authority() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let shell = Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(StatefulPolicy { deny: true })
        .source_filter(StatefulPolicy { deny: true })
        .file_open_filter(StatefulPolicy { deny: true })
        .working_dir(dir.path().to_owned())
        .profile(ProfileLoadBehavior::Skip)
        .rc(RcLoadBehavior::Skip)
        .build()
        .await?;
    let saved = serde_json::to_string(&shell)?;
    let mut restored: Shell<Extensions> = serde_json::from_str(&saved)?;
    assert!(
        restored.cmd_exec_filter().deny,
        "command policy must not become its permissive default"
    );
    assert!(
        restored.source_filter().deny,
        "source policy must not become its permissive default"
    );
    assert!(
        restored.file_open_filter().deny,
        "file policy must not become its permissive default"
    );
    let params = restored.default_exec_params();
    assert!(
        restored
            .run_string("> refused", &brush_core::SourceInfo::default(), &params)
            .await
            .is_err()
    );
    assert!(!dir.path().join("refused").exists());
    Ok(())
}

#[tokio::test]
async fn policy_serialization_round_trips_noop_and_rejects_missing_policies() -> anyhow::Result<()>
{
    let shell = Shell::builder()
        .profile(ProfileLoadBehavior::Skip)
        .rc(RcLoadBehavior::Skip)
        .build()
        .await?;
    let saved = serde_json::to_value(&shell)?;
    let _: Shell = serde_json::from_value(saved.clone())?;
    for name in ["cmd_exec_filter", "source_filter", "file_open_filter"] {
        let mut incomplete = saved.clone();
        incomplete.as_object_mut().unwrap().remove(name);
        assert!(
            serde_json::from_value::<Shell>(incomplete).is_err(),
            "missing {name} cannot silently restore defaults"
        );
    }
    Ok(())
}
