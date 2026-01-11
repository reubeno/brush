//! Test harness library for brush shell integration tests.
//!
//! This crate provides a unified framework for running YAML-based integration tests
//! that support both oracle-based comparison (comparing brush output against bash)
//! and expectation-based testing (inline expectations or insta snapshots).
//!
//! # Modes of Operation
//!
//! 1. **Oracle comparison**: Runs both an oracle shell (e.g., bash) and the test shell (brush),
//!    comparing their outputs. This is the traditional compatibility testing mode.
//!
//! 2. **Expectation-based**: Runs only the test shell and compares against inline expectations
//!    specified in the YAML or against insta snapshots.
//!
//! 3. **Hybrid**: Combines both modes - runs oracle comparison AND validates against expectations.
//!    Both must pass for the test to succeed.

#![cfg(any(unix, windows))]

mod comparison;
mod config;
mod execution;
mod reporting;
mod runner;
mod testcase;
pub mod util;

pub use comparison::{
    DirComparison, DirComparisonEntry, DurationComparison, ExitStatusComparison,
    ExpectationComparison, OracleComparison, SingleExpectationComparison, SnapshotResult,
    StringComparison, TestComparison,
};
pub use config::{
    OracleConfig, OutputFormat, RunnerConfig, ShellConfig, TestMode, TestOptions, WhichShell,
};
pub use execution::RunResult;
pub use runner::TestRunner;
pub use testcase::{ShellInvocation, TestCase, TestCaseSet, TestFile};
