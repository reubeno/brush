//! Brush-only test harness.
//!
//! This test harness runs YAML-based test cases with inline expectations
//! or insta snapshots, without comparing against an oracle shell.

#![cfg(any(unix, windows))]

use anyhow::Result;
use brush_test_harness::{
    RunnerConfig, ShellConfig, TestMode, TestOptions, TestRunner, WhichShell,
};
use clap::Parser;
use std::path::{Path, PathBuf};

fn create_test_shell_config(options: &TestOptions) -> ShellConfig {
    let mut default_args = vec![
        "--norc".into(),
        "--noprofile".into(),
        "--no-config".into(),
        "--input-backend=basic".into(),
        "--disable-bracketed-paste".into(),
        "--disable-color".into(),
    ];

    // Add any additional brush args specified.
    options.brush_args.split_whitespace().for_each(|arg| {
        default_args.push(arg.into());
    });

    ShellConfig {
        which: WhichShell::ShellUnderTest(PathBuf::from(&options.brush_path)),
        default_args,
        default_path_var: options.test_path_var.clone(),
    }
}

async fn run_brush_tests(mut options: TestOptions) -> Result<bool> {
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

    // Resolve test cases directory (in cases/brush/).
    let test_cases_dir = options.test_cases_path.as_deref().map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases/brush"),
        |p| p.to_owned(),
    );

    let test_shell = create_test_shell_config(&options);

    let config = RunnerConfig::new(PathBuf::from(&options.brush_path), test_cases_dir)
        .with_mode(TestMode::Expectation);

    let config = RunnerConfig {
        test_shell,
        ..config
    };

    let runner = TestRunner::new(config, options);
    runner.run().await
}

fn main() -> Result<()> {
    let unparsed_args: Vec<_> = std::env::args().collect();
    let options = TestOptions::parse_from(unparsed_args);

    let success = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(32)
        .build()?
        .block_on(run_brush_tests(options))?;

    if !success {
        std::process::exit(1);
    }

    Ok(())
}
