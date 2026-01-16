//! Compatibility test harness for brush shell.
//!
//! This test harness runs YAML-based test cases comparing brush output against
//! bash (the oracle shell) to validate compatibility.

#![cfg(any(unix, windows))]

use anyhow::Result;
use brush_test_harness::{
    OracleConfig, RunnerConfig, ShellConfig, TestMode, TestOptions, TestRunner, WhichShell,
};
use clap::Parser;
use std::path::{Path, PathBuf};

const BASH_CONFIG_NAME: &str = "bash";
const SH_CONFIG_NAME: &str = "sh";

fn get_bash_version_str(bash_path: &Path) -> Result<String> {
    brush_test_harness::util::get_bash_version_str(bash_path)
}

fn create_bash_oracle(options: &TestOptions) -> Result<OracleConfig> {
    let bash_version_str = get_bash_version_str(&options.bash_path)?;
    if options.verbose {
        eprintln!("Detected bash version: {bash_version_str}");
    }

    Ok(OracleConfig {
        name: String::from(BASH_CONFIG_NAME),
        shell: ShellConfig {
            which: WhichShell::NamedShell(options.bash_path.clone()),
            default_args: vec![String::from("--norc"), String::from("--noprofile")],
            default_path_var: options.test_path_var.clone(),
        },
        version_str: Some(bash_version_str),
    })
}

fn create_sh_oracle(options: &TestOptions) -> OracleConfig {
    OracleConfig {
        name: String::from(SH_CONFIG_NAME),
        shell: ShellConfig {
            which: WhichShell::NamedShell(PathBuf::from("sh")),
            default_args: vec![],
            default_path_var: options.test_path_var.clone(),
        },
        version_str: None,
    }
}

fn create_test_shell_config(options: &TestOptions, oracle_name: &str) -> ShellConfig {
    let mut default_args = vec![
        "--norc".into(),
        "--noprofile".into(),
        "--no-config".into(),
        "--input-backend=basic".into(),
        "--disable-bracketed-paste".into(),
        "--disable-color".into(),
    ];

    // Add --sh flag when testing against sh oracle
    if oracle_name == SH_CONFIG_NAME {
        default_args.insert(0, "--sh".into());
    }

    ShellConfig {
        which: WhichShell::ShellUnderTest(PathBuf::from(&options.brush_path)),
        default_args,
        default_path_var: options.test_path_var.clone(),
    }
}

async fn run_compat_tests(mut options: TestOptions) -> Result<bool> {
    // Resolve path to the shell-under-test.
    if options.brush_path.is_empty() {
        options.brush_path = assert_cmd::cargo::cargo_bin!("brush")
            .to_string_lossy()
            .to_string();
    }
    if !Path::new(&options.brush_path).exists() {
        return Err(anyhow::anyhow!(
            "brush binary not found: {}",
            options.brush_path
        ));
    }

    // Resolve test cases directory (now under compat/).
    let test_cases_dir = options.test_cases_path.as_deref().map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/compat"),
        |p| p.to_owned(),
    );

    let mut all_passed = true;

    // Run tests for each enabled config
    if options.should_enable_config(BASH_CONFIG_NAME, &[BASH_CONFIG_NAME]) {
        let oracle = create_bash_oracle(&options)?;
        let test_shell = create_test_shell_config(&options, &oracle.name);

        let config = RunnerConfig::new(PathBuf::from(&options.brush_path), test_cases_dir.clone())
            .with_oracle(oracle)
            .with_mode(TestMode::Oracle);

        let config = RunnerConfig {
            test_shell,
            ..config
        };

        let runner = TestRunner::new(config, options.clone());
        if !runner.run().await? {
            all_passed = false;
        }
    }

    if options.should_enable_config(SH_CONFIG_NAME, &[BASH_CONFIG_NAME]) {
        let oracle = create_sh_oracle(&options);
        let test_shell = create_test_shell_config(&options, &oracle.name);

        let config = RunnerConfig::new(PathBuf::from(&options.brush_path), test_cases_dir.clone())
            .with_oracle(oracle)
            .with_mode(TestMode::Oracle);

        let config = RunnerConfig {
            test_shell,
            ..config
        };

        let runner = TestRunner::new(config, options.clone());
        if !runner.run().await? {
            all_passed = false;
        }
    }

    Ok(all_passed)
}

fn main() -> Result<()> {
    let unparsed_args: Vec<_> = std::env::args().collect();
    let options = TestOptions::parse_from(unparsed_args);

    let success = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(32)
        .build()?
        .block_on(run_compat_tests(options))?;

    if !success {
        std::process::exit(1);
    }

    Ok(())
}
