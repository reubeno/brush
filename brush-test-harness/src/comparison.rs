//! Comparison types for test results.

use std::{io::Read, path::PathBuf, process::ExitStatus};

/// Comparison of durations between oracle and test runs.
#[derive(Default)]
pub struct DurationComparison {
    /// Duration of the oracle run.
    pub oracle: std::time::Duration,
    /// Duration of the test run.
    pub test: std::time::Duration,
}

/// Comparison of exit statuses.
pub enum ExitStatusComparison {
    /// Exit status was ignored.
    Ignored,
    /// Exit statuses match.
    Same(ExitStatus),
    /// Exit statuses differ.
    TestDiffers {
        /// Exit status from the test shell.
        test_exit_status: ExitStatus,
        /// Exit status from the oracle shell.
        oracle_exit_status: ExitStatus,
    },
}

impl ExitStatusComparison {
    /// Returns whether this comparison indicates a failure.
    pub const fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::TestDiffers {
                test_exit_status: _,
                oracle_exit_status: _
            }
        )
    }
}

/// Comparison of string outputs (stdout/stderr).
pub enum StringComparison {
    /// Output was ignored.
    Ignored {
        /// Output from the test shell.
        test_string: String,
        /// Output from the oracle shell.
        oracle_string: String,
    },
    /// Outputs match.
    Same(String),
    /// Outputs differ.
    TestDiffers {
        /// Output from the test shell.
        test_string: String,
        /// Output from the oracle shell.
        oracle_string: String,
    },
}

impl StringComparison {
    /// Returns whether this comparison indicates a failure.
    pub const fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::TestDiffers {
                test_string: _,
                oracle_string: _
            }
        )
    }
}

/// A single entry in a directory comparison.
pub enum DirComparisonEntry {
    /// File exists only in the left (oracle) directory.
    LeftOnly(PathBuf),
    /// File exists only in the right (test) directory.
    RightOnly(PathBuf),
    /// Files differ between directories.
    Different(PathBuf, String, PathBuf, String),
}

/// Comparison of directory contents.
pub enum DirComparison {
    /// Directory comparison was ignored.
    Ignored,
    /// Directory contents match.
    Same,
    /// Directory contents differ.
    TestDiffers(Vec<DirComparisonEntry>),
}

impl DirComparison {
    /// Returns whether this comparison indicates a failure.
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::TestDiffers(_))
    }
}

/// Full comparison between oracle and test shell runs.
pub struct OracleComparison {
    /// Comparison of exit statuses.
    pub exit_status: ExitStatusComparison,
    /// Comparison of stdout.
    pub stdout: StringComparison,
    /// Comparison of stderr.
    pub stderr: StringComparison,
    /// Comparison of temporary directory contents.
    pub temp_dir: DirComparison,
    /// Comparison of durations.
    pub duration: DurationComparison,
}

impl OracleComparison {
    /// Returns whether this comparison indicates a failure.
    pub const fn is_failure(&self) -> bool {
        self.exit_status.is_failure()
            || self.stdout.is_failure()
            || self.stderr.is_failure()
            || self.temp_dir.is_failure()
    }

    /// Creates an ignored comparison (all fields ignored).
    pub fn ignored() -> Self {
        Self {
            exit_status: ExitStatusComparison::Ignored,
            stdout: StringComparison::Ignored {
                test_string: String::new(),
                oracle_string: String::new(),
            },
            stderr: StringComparison::Ignored {
                test_string: String::new(),
                oracle_string: String::new(),
            },
            temp_dir: DirComparison::Ignored,
            duration: DurationComparison::default(),
        }
    }
}

/// Comparison of a single expectation.
#[derive(Debug)]
pub enum SingleExpectationComparison {
    /// Expectation was not specified (ignored).
    NotSpecified,
    /// Actual matches expected.
    Matches,
    /// Actual differs from expected.
    Differs {
        /// The expected value.
        expected: String,
        /// The actual value.
        actual: String,
    },
}

impl SingleExpectationComparison {
    /// Returns whether this comparison indicates a failure.
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Differs { .. })
    }
}

/// Comparison against inline expectations.
#[derive(Debug)]
pub struct ExpectationComparison {
    /// Comparison of exit code.
    pub exit_code: SingleExpectationComparison,
    /// Comparison of stdout.
    pub stdout: SingleExpectationComparison,
    /// Comparison of stderr.
    pub stderr: SingleExpectationComparison,
    /// Whether snapshot comparison was used.
    pub snapshot_used: bool,
    /// Snapshot comparison result (if used).
    pub snapshot_result: Option<SnapshotResult>,
}

impl ExpectationComparison {
    /// Creates an empty expectation comparison (all not specified).
    pub const fn not_specified() -> Self {
        Self {
            exit_code: SingleExpectationComparison::NotSpecified,
            stdout: SingleExpectationComparison::NotSpecified,
            stderr: SingleExpectationComparison::NotSpecified,
            snapshot_used: false,
            snapshot_result: None,
        }
    }

    /// Returns whether this comparison indicates a failure.
    pub fn is_failure(&self) -> bool {
        self.exit_code.is_failure()
            || self.stdout.is_failure()
            || self.stderr.is_failure()
            || self
                .snapshot_result
                .as_ref()
                .is_some_and(|r| r.is_failure())
    }

    /// Returns whether any expectations were checked.
    pub const fn has_any_checks(&self) -> bool {
        !matches!(self.exit_code, SingleExpectationComparison::NotSpecified)
            || !matches!(self.stdout, SingleExpectationComparison::NotSpecified)
            || !matches!(self.stderr, SingleExpectationComparison::NotSpecified)
            || self.snapshot_used
    }
}

/// Result of a snapshot comparison.
#[derive(Debug)]
pub enum SnapshotResult {
    /// Snapshot matches.
    Matches,
    /// Snapshot differs (new snapshot created or update needed).
    Differs {
        /// Description of the difference.
        message: String,
    },
}

impl SnapshotResult {
    /// Returns whether this result indicates a failure.
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Differs { .. })
    }
}

/// Combined test comparison result.
pub struct TestComparison {
    /// Oracle comparison (if oracle mode was used).
    pub oracle: Option<OracleComparison>,
    /// Expectation comparison (if expectations were defined).
    pub expectation: ExpectationComparison,
    /// Duration of the test run.
    pub duration: std::time::Duration,
}

impl TestComparison {
    /// Returns whether this comparison indicates a failure.
    pub fn is_failure(&self) -> bool {
        self.oracle.as_ref().is_some_and(|o| o.is_failure()) || self.expectation.is_failure()
    }

    /// Creates a skipped comparison.
    pub fn skipped() -> Self {
        Self {
            oracle: None,
            expectation: ExpectationComparison::not_specified(),
            duration: std::time::Duration::default(),
        }
    }
}

/// Compares two strings, optionally ignoring whitespace.
pub fn output_matches(oracle: &str, test: &str, ignore_whitespace: bool) -> bool {
    if ignore_whitespace {
        let whitespace_re = regex::Regex::new(r"\s+").unwrap();

        let cleaned_oracle = whitespace_re.replace_all(oracle, " ").to_string();
        let cleaned_test = whitespace_re.replace_all(test, " ").to_string();

        cleaned_oracle == cleaned_test
    } else {
        oracle == test
    }
}

/// Compares directory contents between oracle and test.
pub fn diff_dirs(
    oracle_path: &std::path::Path,
    test_path: &std::path::Path,
) -> anyhow::Result<DirComparison> {
    use std::collections::HashMap;
    use std::fs;

    fn get_dir_entries(
        dir_path: &std::path::Path,
    ) -> anyhow::Result<HashMap<String, fs::FileType>> {
        let mut entries = HashMap::new();
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let filename = entry.file_name().to_string_lossy().to_string();

            // Ignore raw coverage profile data files.
            if filename.ends_with(".profraw") {
                continue;
            }

            entries.insert(filename, file_type);
        }

        Ok(entries)
    }

    let mut entries = vec![];

    let oracle_entries = get_dir_entries(oracle_path)?;
    let test_entries = get_dir_entries(test_path)?;

    // Look through all the files in the oracle directory
    for (filename, file_type) in &oracle_entries {
        if !test_entries.contains_key(filename) {
            for left_only_file in walkdir::WalkDir::new(oracle_path.join(filename)) {
                let entry = left_only_file?;
                let left_only_path = entry.path();
                entries.push(DirComparisonEntry::LeftOnly(left_only_path.to_owned()));
            }

            continue;
        }

        let oracle_file_path = oracle_path.join(filename);
        let test_file_path = test_path.join(filename);

        if file_type.is_file() {
            let mut oracle_file = std::fs::OpenOptions::new()
                .read(true)
                .open(&oracle_file_path)?;
            let mut oracle_bytes = vec![];
            oracle_file.read_to_end(&mut oracle_bytes)?;

            let mut test_file = std::fs::OpenOptions::new()
                .read(true)
                .open(&test_file_path)?;
            let mut test_bytes = vec![];
            test_file.read_to_end(&mut test_bytes)?;

            if oracle_bytes != test_bytes {
                let oracle_display_text = String::from_utf8_lossy(&oracle_bytes);
                let test_display_text = String::from_utf8_lossy(&test_bytes);

                entries.push(DirComparisonEntry::Different(
                    oracle_file_path,
                    oracle_display_text.to_string(),
                    test_file_path,
                    test_display_text.to_string(),
                ));
            }
        } else if file_type.is_dir() {
            let subdir_comparison =
                diff_dirs(oracle_file_path.as_path(), test_file_path.as_path())?;
            if let DirComparison::TestDiffers(subdir_entries) = subdir_comparison {
                entries.extend(subdir_entries);
            }
        }
    }

    for (filename, file_type) in &test_entries {
        if oracle_entries.contains_key(filename) {
            continue;
        }

        if file_type.is_dir() {
            for right_only_file in walkdir::WalkDir::new(test_path.join(filename)) {
                let entry = right_only_file?;
                let right_only_path = entry.path();
                entries.push(DirComparisonEntry::RightOnly(right_only_path.to_owned()));
            }
        } else {
            entries.push(DirComparisonEntry::RightOnly(test_path.join(filename)));
        }
    }

    if entries.is_empty() {
        Ok(DirComparison::Same)
    } else {
        Ok(DirComparison::TestDiffers(entries))
    }
}
