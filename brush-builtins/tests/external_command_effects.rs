//! External-command transformations must reach the real child process.
//! Self-executable children provide evidence without depending on a host shell.
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::path::{Path, PathBuf};

use anyhow::Result;
use brush_core::extensions::{DefaultErrorFormatter, ShellExtensionsImpl};
use brush_core::filter::{CmdExecFilter, ExternalCmdOutput, ExternalCmdParams, PreFilterResult};
use brush_core::{ProfileLoadBehavior, RcLoadBehavior, Shell};

const REPORT_ARG: &str = "brush_effect_report=";
#[cfg(all(unix, feature = "builtin.exec"))]
const ROOT_ARG: &str = "brush_effect_root=";
#[cfg(all(unix, feature = "builtin.exec"))]
const CLEAR_ARG: &str = "brush_effect_clear=";

#[derive(Clone, Default)]
struct Transform {
    cwd: PathBuf,
    clear: bool,
}

#[allow(
    clippy::unused_async_trait_impl,
    reason = "Test hooks defer their effects until polled"
)]
impl CmdExecFilter for Transform {
    async fn pre_external_cmd<'a, SE: brush_core::ShellExtensions>(
        &self,
        mut params: ExternalCmdParams<'a, SE>,
    ) -> PreFilterResult<ExternalCmdParams<'a, SE>, ExternalCmdOutput> {
        if self.clear {
            params.command.clear_env();
        }
        params
            .command
            .env("BRUSH_EFFECT_REPLACE", "filtered")
            .env("BRUSH_EFFECT_ADD", "added")
            .set_current_dir(&self.cwd);
        PreFilterResult::Continue(params)
    }
}

type Extensions = ShellExtensionsImpl<DefaultErrorFormatter, Transform>;

fn argument(prefix: &str) -> Result<String> {
    std::env::args()
        .find_map(|argument| argument.strip_prefix(prefix).map(str::to_owned))
        .ok_or_else(|| anyhow::anyhow!("missing fixture argument {prefix}"))
}

fn expected_report(cwd: &Path, clear: bool) -> String {
    format!(
        "cwd={}\nreplace=filtered\nadd=added\npreserve={}\n",
        cwd.display(),
        if clear { "<absent>" } else { "original" }
    )
}

/// The report path travels in argv, so broken environment transformations still
/// produce inspectable evidence. Extra positional libtest filters are harmless.
#[test]
#[ignore = "self-executable child fixture"]
fn external_effects_report_child() -> Result<()> {
    let variable = |name| std::env::var(name).unwrap_or_else(|_| "<absent>".to_owned());
    let cwd = std::env::current_dir()?.canonicalize()?;
    std::fs::write(
        argument(REPORT_ARG)?,
        format!(
            "cwd={}\nreplace={}\nadd={}\npreserve={}\n",
            cwd.display(),
            variable("BRUSH_EFFECT_REPLACE"),
            variable("BRUSH_EFFECT_ADD"),
            variable("BRUSH_EFFECT_PRESERVE")
        ),
    )?;
    Ok(())
}

fn report_source(report: &Path) -> Result<String> {
    let executable = std::env::current_exe()?;
    let report_argument = format!("{REPORT_ARG}{}", report.display());
    Ok(format!(
        "{} --ignored --exact external_effects_report_child {}",
        brush_core::escape::single_quote(&executable.to_string_lossy()),
        brush_core::escape::single_quote(&report_argument)
    ))
}

async fn filtered_shell(root: &Path, clear: bool) -> Result<Shell<Extensions>> {
    let original = root.join("original");
    let target = root.join("filtered");
    std::fs::create_dir_all(&original)?;
    std::fs::create_dir_all(&target)?;
    let mut shell = Shell::builder_with_extensions::<Extensions>()
        .cmd_exec_filter(Transform { cwd: target, clear })
        .builtins(brush_builtins::default_builtins::<Extensions>(
            brush_builtins::BuiltinSet::BashMode,
        ))
        .working_dir(original)
        .profile(ProfileLoadBehavior::Skip)
        .rc(RcLoadBehavior::Skip)
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .build()
        .await?;
    for name in ["BRUSH_EFFECT_REPLACE", "BRUSH_EFFECT_PRESERVE"] {
        let mut variable = brush_core::variables::ShellVariable::new("original");
        variable.export();
        shell.set_env_global(name, variable)?;
    }
    Ok(shell)
}

async fn assert_external_effects(clear: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let report = root.join("report");
    let mut shell = filtered_shell(&root, clear).await?;
    let result = shell
        .run_string(
            &report_source(&report)?,
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await?;
    assert!(result.is_success(), "self-executable child must run");
    assert_eq!(
        std::fs::read_to_string(report)?,
        expected_report(&root.join("filtered"), clear),
        "the spawned child must receive the filter's final env and cwd"
    );
    Ok(())
}

#[tokio::test]
async fn external_filter_env_and_cwd_reach_the_child() -> Result<()> {
    assert_external_effects(false).await
}

#[tokio::test]
async fn external_filter_clear_env_removes_shell_exports() -> Result<()> {
    assert_external_effects(true).await
}

#[test]
fn report_child_fixture_records_actual_env_and_cwd() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let report = root.join("report");
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--ignored", "--exact", "external_effects_report_child"])
        .arg(format!("{REPORT_ARG}{}", report.display()))
        .env_clear()
        .env("BRUSH_EFFECT_REPLACE", "filtered")
        .env("BRUSH_EFFECT_ADD", "added")
        .current_dir(&root)
        .output()?;
    assert!(
        output.status.success(),
        "self-executable fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(report)?,
        expected_report(&root, true)
    );
    Ok(())
}

// exec replaces the process image; isolate it in another self-executable child
// so an implementation bug can neither replace nor terminate the test runner.
#[cfg(all(unix, feature = "builtin.exec"))]
#[tokio::test]
#[ignore = "isolated exec driver fixture"]
async fn external_effects_exec_driver() -> Result<()> {
    let root = PathBuf::from(argument(ROOT_ARG)?);
    let clear = argument(CLEAR_ARG)? == "true";
    let report = PathBuf::from(argument(REPORT_ARG)?);
    let mut shell = filtered_shell(&root, clear).await?;
    shell
        .run_string(
            &format!("exec {}", report_source(&report)?),
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await?;
    anyhow::bail!("exec returned instead of replacing its isolated driver")
}

#[cfg(all(unix, feature = "builtin.exec"))]
fn assert_exec_effects(clear: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let report = root.join("report");
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--ignored", "--exact", "external_effects_exec_driver"])
        .arg(format!("{REPORT_ARG}{}", report.display()))
        .arg(format!("{ROOT_ARG}{}", root.display()))
        .arg(format!("{CLEAR_ARG}{clear}"))
        .output()?;
    assert!(
        output.status.success(),
        "isolated exec driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(report)?,
        expected_report(&root.join("filtered"), clear),
        "exec must receive the same filtered env and cwd as ordinary external execution"
    );
    Ok(())
}

#[cfg(all(unix, feature = "builtin.exec"))]
#[test]
fn exec_filter_env_and_cwd_reach_the_replacement() -> Result<()> {
    assert_exec_effects(false)
}

#[cfg(all(unix, feature = "builtin.exec"))]
#[test]
fn exec_filter_clear_env_removes_shell_exports() -> Result<()> {
    assert_exec_effects(true)
}
