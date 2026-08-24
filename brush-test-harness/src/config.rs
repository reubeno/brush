//! Configuration types for the test harness.

use bpaf::Parser;
use std::str::FromStr;
use std::{collections::HashSet, ffi::OsString, path::PathBuf};

/// Which shell to use for a test.
#[derive(Clone, Debug)]
pub enum WhichShell {
    /// The shell under test (brush).
    ShellUnderTest(PathBuf),
    /// A named shell (e.g., bash, sh).
    NamedShell(PathBuf),
}

/// Configuration for a shell.
#[derive(Clone, Debug)]
pub struct ShellConfig {
    /// Which shell this is.
    pub which: WhichShell,
    /// Default arguments to pass to this shell.
    pub default_args: Vec<String>,
    /// Default PATH variable for this shell.
    pub default_path_var: Option<String>,
    /// Optional launcher command to prepend (e.g., `["wasmtime", "run", "--"]` for wasm
    /// targets). The first element is the program to execute; the rest are leading arguments
    /// inserted before the shell binary path.
    pub launcher: Option<Vec<String>>,
}

impl ShellConfig {
    /// Computes the PATH variable to use for tests.
    pub fn compute_test_path_var(&self) -> OsString {
        let mut dirs = vec![];

        // Start with any default we were provided.
        if let Some(default_path_var) = &self.default_path_var {
            dirs.extend(std::env::split_paths(default_path_var));
        }

        // Add hard-coded paths that will work on *most* Unix-like systems.
        dirs.extend([
            "/usr/local/sbin".into(),
            "/usr/local/bin".into(),
            "/usr/sbin".into(),
            "/usr/bin".into(),
            "/sbin".into(),
            "/bin".into(),
        ]);

        // Handle systems that store their standard POSIX binaries elsewhere.
        // For example, NixOS has an interesting set of paths that must be consulted.
        if let Some(host_path) = std::env::var_os("PATH") {
            for path in std::env::split_paths(&host_path) {
                if !dirs.contains(&path) && path.join("sh").is_file() {
                    dirs.push(path);
                }
            }
        }

        std::env::join_paths(dirs).unwrap_or_else(|_| PathBuf::from("").into())
    }
}

/// Configuration for the oracle shell (e.g., bash).
#[derive(Clone, Debug)]
pub struct OracleConfig {
    /// Name of this oracle configuration (e.g., "bash", "sh").
    pub name: String,
    /// Shell configuration for the oracle.
    pub shell: ShellConfig,
    /// Version string of the oracle.
    pub version_str: Option<String>,
}

/// The mode in which to run tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TestMode {
    /// Compare test shell output against an oracle shell.
    #[default]
    Oracle,
    /// Validate against inline expectations or snapshots only.
    Expectation,
    /// Both oracle comparison and expectation validation.
    Hybrid,
}

/// Configuration for the test runner.
#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// The test mode to use.
    pub mode: TestMode,
    /// Configuration for the oracle shell (if using oracle mode).
    pub oracle: Option<OracleConfig>,
    /// Configuration for the test shell (brush).
    pub test_shell: ShellConfig,
    /// Directory containing test case YAML files.
    pub test_cases_dir: PathBuf,
    /// Directory for storing snapshots (relative to test case YAML files).
    pub snapshot_dir_name: String,
    /// Host OS ID (for filtering incompatible tests).
    pub host_os_id: Option<String>,
    /// Active runtime platform tags (e.g., "wasi", "wasm"). Tests that
    /// declare any of these in `incompatible_platforms` will be skipped.
    pub platform_tags: HashSet<String>,
}

impl RunnerConfig {
    /// Creates a new runner config with minimal safe defaults.
    ///
    /// N.B. Callers typically override `test_shell` via
    /// `TestOptions::create_test_shell_config()`, which adds
    /// platform-appropriate flags like `--input-backend=basic`.
    pub fn new(test_shell_path: PathBuf, test_cases_dir: PathBuf) -> Self {
        Self {
            mode: TestMode::Expectation,
            oracle: None,
            test_shell: ShellConfig {
                which: WhichShell::ShellUnderTest(test_shell_path),
                default_args: vec![
                    "--norc".into(),
                    "--noprofile".into(),
                    "--no-config".into(),
                    "--disable-bracketed-paste".into(),
                    "--disable-color".into(),
                ],
                default_path_var: None,
                launcher: None,
            },
            test_cases_dir,
            snapshot_dir_name: String::from("snaps"),
            host_os_id: crate::util::get_host_os_id(),
            platform_tags: HashSet::new(),
        }
    }

    /// Sets the active runtime platform tags.
    #[must_use]
    pub fn with_platform_tags(mut self, tags: HashSet<String>) -> Self {
        self.platform_tags = tags;
        self
    }

    /// Sets the oracle configuration, enabling oracle comparison mode.
    #[must_use]
    pub fn with_oracle(mut self, oracle: OracleConfig) -> Self {
        self.oracle = Some(oracle);
        self.mode = TestMode::Oracle;
        self
    }

    /// Sets the test mode.
    #[must_use]
    pub const fn with_mode(mut self, mode: TestMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the snapshot directory name.
    #[must_use]
    pub fn with_snapshot_dir_name(mut self, name: impl Into<String>) -> Self {
        self.snapshot_dir_name = name.into();
        self
    }

    /// Sets the default PATH variable for the test shell.
    #[must_use]
    pub fn with_test_path_var(mut self, path_var: Option<String>) -> Self {
        self.test_shell.default_path_var = path_var;
        self
    }
}

/// Output format for test results.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable colored output.
    #[default]
    Pretty,
    /// `JUnit` XML format.
    Junit,
    /// Minimal output.
    Terse,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pretty" => Ok(Self::Pretty),
            "junit" => Ok(Self::Junit),
            "terse" => Ok(Self::Terse),
            _ => Err(format!("invalid output format: `{s}`")),
        }
    }
}

/// Command-line options for the test harness.
#[derive(Clone, Debug)]
pub struct TestOptions {
    /// Output format for test results.
    pub format: OutputFormat,

    /// Display full details on known failures.
    pub display_known_failure_details: bool,

    /// Display details regarding successful test cases.
    pub verbose: bool,

    /// Enable a specific configuration.
    pub enabled_configs: Vec<String>,

    /// List available tests without running them.
    pub list_tests_only: bool,

    /// Exactly match filters (not just substring match).
    pub exact_match: bool,

    /// Optionally specify a non-default path for bash.
    pub bash_path: PathBuf,

    /// Optionally specify a non-default path for brush.
    pub brush_path: String,

    /// Optionally specify additional arguments for brush.
    pub brush_args: String,

    /// Optionally specify a launcher command to prepend when invoking brush
    /// (e.g., "wasmtime run --" to execute a wasm build under wasmtime).
    pub brush_launcher: String,

    /// Runtime platform tags (e.g., "wasi", "wasm") describing the
    /// environment in which brush is being executed.
    pub brush_platform_tags: Vec<String>,

    /// Optionally specify path to test cases.
    pub test_cases_path: Option<PathBuf>,

    /// Optionally specify PATH variable to use in shells.
    pub test_path_var: Option<String>,

    /// Show output from test cases (for compatibility only, has no effect).
    #[allow(dead_code, reason = "accepted for compatibility only")]
    pub show_output: bool,

    /// Capture output? (for compatibility only, has no effect).
    #[allow(dead_code, reason = "accepted for compatibility only")]
    pub no_capture: bool,

    /// Colorize output? (for compatibility only, has no effect).
    #[allow(dead_code, reason = "accepted for compatibility only")]
    pub color: Option<String>,

    /// Run skipped tests only.
    pub skipped_tests_only: bool,

    /// Unstable flags (for compatibility only, has no effect).
    #[allow(dead_code, reason = "accepted for compatibility only")]
    pub unstable_flag: Vec<String>,

    /// Patterns for tests to be excluded.
    pub exclude_filters: Vec<String>,

    /// Patterns for tests to be included.
    pub include_filters: Vec<String>,
}

impl TestOptions {
    /// Returns a parser for the test harness options.
    #[must_use]
    pub fn parser() -> impl Parser<Self> {
        let format = bpaf::long("format")
            .help("Output format for test results.")
            .argument::<OutputFormat>("FORMAT")
            .fallback(OutputFormat::Pretty);
        let display_known_failure_details = bpaf::long("known-failure-details").switch();
        let verbose = bpaf::short('v')
            .long("verbose")
            .help("Display details regarding successful test cases.")
            .env("BRUSH_VERBOSE")
            .switch();
        let enabled_configs = bpaf::long("enable-config")
            .argument::<String>("CONFIG")
            .many()
            .fallback(Vec::new());
        let list_tests_only = bpaf::long("list").switch();
        let exact_match = bpaf::long("exact").switch();

        let bash_path = bpaf::long("bash-path")
            .help("Optionally specify a non-default path for bash.")
            .env("BASH_PATH")
            .argument::<PathBuf>("PATH")
            .fallback(PathBuf::from("bash"));

        let brush_path = bpaf::long("brush-path")
            .help("Optionally specify a non-default path for brush.")
            .env("BRUSH_PATH")
            .argument::<String>("PATH")
            .fallback(String::new());
        let brush_args = bpaf::long("brush-args")
            .help("Additional arguments for brush.")
            .env("BRUSH_ARGS")
            .argument::<String>("ARGS")
            .fallback(String::new());
        let brush_launcher = bpaf::long("brush-launcher")
            .help("Launcher command to prepend when invoking brush.")
            .env("BRUSH_LAUNCHER")
            .argument::<String>("CMD")
            .fallback(String::new());
        // N.B. The environment value is space-separated, matching clap's
        // former `value_delimiter`. `.some()` (not `.many()`) is required so
        // that absence on the CLI lets the environment fallback apply.
        let brush_platform_tags = bpaf::long("brush-platform-tags")
            .help("Runtime platform tags describing the execution environment; test cases declaring any of these in `incompatible_platforms` will be skipped.")
            .env("BRUSH_PLATFORM_TAGS")
            .argument::<String>("TAGS")
            .some("TAGS")
            .parse(|tags: Vec<String>| {
                Ok::<Vec<String>, String>(
                    tags.iter()
                        .flat_map(|t| t.split_whitespace().map(str::to_owned))
                        .collect(),
                )
            })
            .fallback(Vec::new());
        let test_cases_path = bpaf::long("test-cases-path")
            .help("Optionally specify path to test cases.")
            .env("BRUSH_TEST_CASES")
            .argument::<PathBuf>("PATH")
            .optional();
        let test_path_var = bpaf::long("test-path-var")
            .help("Optionally specify PATH variable to use in shells.")
            .env("BRUSH_TEST_PATH_VAR")
            .argument::<String>("VAR")
            .optional();
        let show_output = bpaf::long("show-output").switch();
        let no_capture = bpaf::long("nocapture").switch();
        let color = bpaf::long("color").argument::<String>("WHEN").optional();
        let skipped_tests_only = bpaf::long("ignored").switch();
        let unstable_flag = bpaf::short('Z')
            .argument::<String>("FLAG")
            .many()
            .fallback(Vec::new());
        let exclude_filters = bpaf::long("skip")
            .argument::<String>("PATTERN")
            .many()
            .fallback(Vec::new());
        let include_filters = bpaf::positional::<String>("FILTERS")
            .many()
            .fallback(Vec::new());

        bpaf::construct!(TestOptions {
            format,
            display_known_failure_details,
            verbose,
            enabled_configs,
            list_tests_only,
            exact_match,
            bash_path,
            brush_path,
            brush_args,
            brush_launcher,
            brush_platform_tags,
            test_cases_path,
            test_path_var,
            show_output,
            no_capture,
            color,
            skipped_tests_only,
            unstable_flag,
            exclude_filters,
            include_filters,
        })
    }

    /// Parses the test harness options from the given arguments.
    ///
    /// # Arguments
    ///
    /// * `args` - The arguments, including the program name.
    ///
    /// # Panics
    ///
    /// Panics on invalid usage, printing the relevant message first. Help and
    /// version requests print to stdout and exit successfully.
    pub fn parse_from<S: AsRef<str>>(args: impl IntoIterator<Item = S>) -> Self {
        let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        // N.B. The first argument is the program name.
        let rest = args.get(1..).unwrap_or(&[]).to_vec();

        match Self::parser().to_options().run_inner(rest.as_slice()) {
            Ok(options) => options,
            Err(failure @ bpaf::ParseFailure::Stdout(..)) => {
                println!("{}", failure.unwrap_stdout());
                std::process::exit(0);
            }
            Err(failure) => {
                eprintln!("{}", failure.unwrap_stderr());
                std::process::exit(2);
            }
        }
    }

    /// Returns the configured platform tags as a set.
    pub fn platform_tags(&self) -> HashSet<String> {
        self.brush_platform_tags.iter().cloned().collect()
    }

    /// Builds the default `ShellConfig` for the shell under test based on
    /// the common options (path, launcher, platform tags, extra args).
    ///
    /// Resolves the launcher binary to an absolute path (if one is
    /// configured) because the test harness clears env vars — including
    /// `PATH` — before spawning child processes.
    pub fn create_test_shell_config(&self) -> anyhow::Result<ShellConfig> {
        let mut default_args: Vec<String> = vec![
            "--norc".into(),
            "--noprofile".into(),
            "--no-config".into(),
            "--disable-bracketed-paste".into(),
            "--disable-color".into(),
        ];

        // Use the basic input backend for native builds. WASI builds are
        // compiled with `--features minimal` which doesn't include the basic
        // backend, so passing this flag would cause a startup error. Omitting
        // it lets brush pick its own default (Minimal on wasm targets).
        if !self.platform_tags().contains("wasi") {
            default_args.push("--input-backend=basic".into());
        }

        // Append any additional brush args specified by the caller.
        self.brush_args.split_whitespace().for_each(|arg| {
            default_args.push(arg.into());
        });

        let launcher = if self.brush_launcher.is_empty() {
            None
        } else {
            let mut tokens: Vec<String> = self
                .brush_launcher
                .split_whitespace()
                .map(Into::into)
                .collect();
            crate::util::resolve_launcher_path(&mut tokens)?;
            Some(tokens)
        };

        Ok(ShellConfig {
            which: WhichShell::ShellUnderTest(PathBuf::from(&self.brush_path)),
            default_args,
            default_path_var: self.test_path_var.clone(),
            launcher,
        })
    }

    /// Returns whether the given config name should be enabled.
    pub fn should_enable_config(&self, config: &str, default_configs: &[&str]) -> bool {
        let enabled_configs = if self.enabled_configs.is_empty() {
            default_configs.iter().map(|s| String::from(*s)).collect()
        } else {
            self.enabled_configs.clone()
        };

        enabled_configs.contains(&config.to_string())
    }

    /// Returns whether a test should run based on include/exclude filters.
    pub fn should_run_test(&self, qualified_name: &str) -> bool {
        if self.include_filters.is_empty() && self.exclude_filters.is_empty() {
            return true;
        }

        // If any include filters were given, then we are in opt-in mode.
        if !self.include_filters.is_empty()
            && !self.test_matches_filters(qualified_name, &self.include_filters)
        {
            return false;
        }

        // In all cases, exclude filters may be used to exclude tests.
        if !self.exclude_filters.is_empty()
            && self.test_matches_filters(qualified_name, &self.exclude_filters)
        {
            return false;
        }

        true
    }

    fn test_matches_filters(&self, qualified_test_name: &str, filters: &[String]) -> bool {
        if self.exact_match {
            filters.iter().any(|f| f == qualified_test_name)
        } else {
            filters
                .iter()
                .any(|filter| qualified_test_name.contains(filter))
        }
    }
}
