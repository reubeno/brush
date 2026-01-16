//! Test case definitions and YAML schema.

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

/// How to invoke the shell for a test case.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub enum ShellInvocation {
    /// Execute the shell binary directly.
    #[default]
    ExecShellBinary,
    /// Execute a script file.
    ExecScript(String),
}

/// A file to create in the test's temporary directory.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestFile {
    /// Relative path to test file within the temp directory.
    pub path: PathBuf,
    /// Contents to seed the file with.
    #[serde(default)]
    pub contents: String,
    /// Optionally provides relative path to the source file
    /// that should be used to populate this file.
    pub source_path: Option<PathBuf>,
    /// Whether the file should be executable.
    #[serde(default)]
    pub executable: bool,
}

/// A single test case.
#[allow(
    clippy::unsafe_derive_deserialize,
    reason = "the unsafe call is unrelated to deserialization"
)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestCase {
    /// Name of the test case.
    pub name: Option<String>,

    /// How to invoke the shell.
    #[serde(default)]
    pub invocation: ShellInvocation,

    /// Command-line arguments to the shell.
    #[serde(default)]
    pub args: Vec<String>,

    /// Command-line arguments to append for the shell-under-test only.
    #[serde(default)]
    pub additional_test_args: Vec<String>,

    /// Default command-line shell arguments that should be *removed*.
    #[serde(default)]
    pub removed_default_args: HashSet<String>,

    /// Environment variables for the shell.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Home directory to set for the test.
    #[serde(default)]
    pub home_dir: Option<PathBuf>,

    /// Whether to skip this test.
    #[serde(default)]
    pub skip: bool,

    /// Whether this test requires a PTY.
    #[serde(default)]
    pub pty: bool,

    /// Input to provide via stdin.
    #[serde(default)]
    pub stdin: Option<String>,

    /// Whether to ignore exit status differences.
    #[serde(default)]
    pub ignore_exit_status: bool,

    /// Whether to ignore stderr differences.
    #[serde(default)]
    pub ignore_stderr: bool,

    /// Whether to ignore stdout differences.
    #[serde(default)]
    pub ignore_stdout: bool,

    /// Whether to normalize whitespace when comparing output.
    #[serde(default)]
    pub ignore_whitespace: bool,

    /// Files to create in the test's temporary directory.
    #[serde(default)]
    pub test_files: Vec<TestFile>,

    /// Whether this test is a known failure.
    #[serde(default)]
    pub known_failure: bool,

    /// Configurations that are incompatible with this test.
    #[serde(default)]
    pub incompatible_configs: HashSet<String>,

    /// Operating systems that are incompatible with this test.
    #[serde(default)]
    pub incompatible_os: HashSet<String>,

    /// Minimum oracle version required for this test.
    #[serde(default)]
    pub min_oracle_version: Option<String>,

    /// Maximum oracle version allowed for this test.
    #[serde(default)]
    pub max_oracle_version: Option<String>,

    /// Timeout for this test in seconds.
    #[serde(default)]
    pub timeout_in_seconds: Option<u64>,

    // ==================== Expectation fields ====================
    /// Expected stdout content (for expectation-based testing).
    #[serde(default)]
    pub expected_stdout: Option<String>,

    /// Expected stderr content (for expectation-based testing).
    #[serde(default)]
    pub expected_stderr: Option<String>,

    /// Expected exit code (for expectation-based testing).
    #[serde(default)]
    pub expected_exit_code: Option<i32>,

    /// Whether to use insta snapshot for this test's expectations.
    #[serde(default)]
    pub snapshot: bool,

    /// Whether to skip oracle comparison even when an oracle is configured.
    #[serde(default)]
    pub skip_oracle: bool,
}

impl TestCase {
    /// Returns whether this test case has any inline expectations defined.
    pub const fn has_inline_expectations(&self) -> bool {
        self.expected_stdout.is_some()
            || self.expected_stderr.is_some()
            || self.expected_exit_code.is_some()
    }

    /// Returns whether this test case uses snapshots for expectations.
    pub const fn uses_snapshot(&self) -> bool {
        self.snapshot
    }

    /// Returns whether this test case has any expectations (inline or snapshot).
    pub const fn has_expectations(&self) -> bool {
        self.has_inline_expectations() || self.uses_snapshot()
    }
}

/// A set of test cases loaded from a single YAML file.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestCaseSet {
    /// Name of the test case set.
    pub name: Option<String>,

    /// The test cases in this set.
    pub cases: Vec<TestCase>,

    /// Common test files applicable to all children test cases.
    #[serde(default)]
    pub common_test_files: Vec<TestFile>,

    /// Configurations that are incompatible with this entire test set.
    #[serde(default)]
    pub incompatible_configs: HashSet<String>,

    /// Directory containing the YAML file (computed at runtime).
    #[serde(skip)]
    pub source_dir: PathBuf,

    /// Path to the YAML file (computed at runtime).
    #[serde(skip)]
    pub source_file: PathBuf,
}
