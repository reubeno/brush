//! Default save retains the installed no-op policies in the actual snapshot.
#![cfg(test)]
#![cfg(feature = "builtin.save")]
#![allow(clippy::panic_in_result_fn)]

#[tokio::test]
async fn default_save_round_trips_without_dropping_policy() -> Result<(), Box<dyn std::error::Error>>
{
    use brush_experimental_builtins::ShellBuilderExt as _;
    let dir = tempfile::tempdir()?;
    let mut shell = brush_core::Shell::builder()
        .working_dir(dir.path().to_owned())
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .profile(brush_core::ProfileLoadBehavior::Skip)
        .rc(brush_core::RcLoadBehavior::Skip)
        .experimental_builtins()
        .build()
        .await?;
    let params = shell.default_exec_params();
    shell
        .run_string(
            "save > snapshot.json",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;
    let saved: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(dir.path().join("snapshot.json"))?)?;
    for name in ["cmd_exec_filter", "source_filter", "file_open_filter"] {
        assert!(saved.as_object().unwrap().contains_key(name));
    }
    let _: brush_core::Shell = serde_json::from_value(saved)?;
    Ok(())
}
