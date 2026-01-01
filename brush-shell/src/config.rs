//! Configuration file support for the brush shell.
//!
//! This module provides TOML-based configuration file loading with the following features:
//! - Forward-compatible: unknown fields are ignored
//! - Graceful degradation: parse errors are logged but don't prevent shell startup
//! - Layered configuration: defaults < config file < command-line arguments

use brush_interactive::UIOptions;
use etcetera::BaseStrategy;
use std::path::{Path, PathBuf};

use crate::args::CommandLineArgs;

/// Root configuration structure for the brush shell.
///
/// All fields are optional to support forward compatibility and partial configuration.
/// Unknown fields in the TOML file are silently ignored.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct Config {
    /// User interface configuration options.
    pub ui: UiConfig,

    /// Experimental features configuration.
    pub experimental: ExperimentalConfig,
}

/// User interface configuration options.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct UiConfig {
    /// Enable syntax highlighting in the input line.
    #[serde(rename = "syntax-highlighting")]
    pub syntax_highlighting: Option<bool>,
}

/// Experimental features configuration.
///
/// These options control unstable features that may change or be removed in future versions.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct ExperimentalConfig {
    /// Enable zsh-style preexec/precmd hooks.
    #[serde(rename = "zsh-hooks")]
    pub zsh_hooks: Option<bool>,

    /// Enable terminal shell integration.
    #[serde(rename = "terminal-shell-integration")]
    pub terminal_shell_integration: Option<bool>,
}

impl Config {
    /// Converts the configuration to [`UIOptions`], merging with CLI arguments.
    ///
    /// Settings are applied with the following priority (highest to lowest):
    /// 1. CLI arguments (if explicitly set, i.e., different from default)
    /// 2. Config file values
    /// 3. Default values
    ///
    /// # Arguments
    ///
    /// * `args` - The parsed command-line arguments
    /// * `default_highlighting` - The compile-time default for syntax highlighting
    #[must_use]
    pub fn to_ui_options(&self, args: &CommandLineArgs, default_highlighting: bool) -> UIOptions {
        let enable_highlighting = merge_bool_setting(
            args.enable_highlighting,
            default_highlighting,
            self.ui.syntax_highlighting,
        );
        let terminal_shell_integration = merge_bool_setting(
            args.terminal_shell_integration,
            false,
            self.experimental.terminal_shell_integration,
        );
        let zsh_style_hooks =
            merge_bool_setting(args.zsh_style_hooks, false, self.experimental.zsh_hooks);

        UIOptions::builder()
            .disable_bracketed_paste(args.disable_bracketed_paste)
            .disable_color(args.disable_color)
            .disable_highlighting(!enable_highlighting)
            .terminal_shell_integration(terminal_shell_integration)
            .zsh_style_hooks(zsh_style_hooks)
            .build()
    }
}

/// Merges a boolean setting from CLI args, config file, and defaults.
///
/// Priority: CLI (if explicitly set) > config file > default.
///
/// Since boolean CLI flags can't distinguish between "explicitly set to false" and
/// "not provided" (both result in `false`), we use a heuristic:
/// - If the CLI value differs from the default, the user explicitly provided it
/// - Otherwise, use the config value if present, or fall back to the default
const fn merge_bool_setting(
    cli_value: bool,
    cli_default: bool,
    config_value: Option<bool>,
) -> bool {
    if cli_value != cli_default {
        // CLI was explicitly set to a non-default value
        cli_value
    } else if let Some(config) = config_value {
        // Use config file value
        config
    } else {
        // Fall back to default
        cli_default
    }
}

/// Result of attempting to load a configuration file.
#[derive(Debug)]
pub struct ConfigLoadResult {
    /// The loaded configuration, or default if loading failed.
    pub config: Config,

    /// The path that was used (or attempted) for loading.
    pub path: Option<PathBuf>,

    /// Any error that occurred during loading.
    pub error: Option<ConfigLoadError>,

    /// Whether the path was explicitly provided by the user (via `--config`).
    /// If true and there's an error, the shell should fail rather than continue.
    pub explicit_path: bool,
}

/// Errors that can occur when loading configuration.
#[derive(Debug)]
pub enum ConfigLoadError {
    /// Failed to read the configuration file.
    Io(std::io::Error),

    /// Failed to parse the TOML content.
    Parse(toml::de::Error),
}

impl std::fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "failed to read config file: {e}"),
            Self::Parse(e) => write!(f, "failed to parse config file: {e}"),
        }
    }
}

impl std::error::Error for ConfigLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
        }
    }
}

/// Returns the default configuration file path for the current platform.
///
/// Uses the XDG Base Directory specification on Linux/macOS and appropriate
/// platform conventions on other systems via the `etcetera` crate.
///
/// Returns `None` if the platform's config directory cannot be determined.
pub fn default_config_path() -> Option<PathBuf> {
    let strategy = etcetera::choose_base_strategy().ok()?;
    Some(strategy.config_dir().join("brush").join("config.toml"))
}

/// Loads configuration from the specified path.
///
/// Returns a `ConfigLoadResult` containing:
/// - The parsed configuration (or default on error)
/// - The path that was used
/// - Any error that occurred
///
/// Note: This function sets `explicit_path` to `false`. Use `load_config` for
/// proper handling of explicit vs. default paths.
pub fn load_from_path(path: &Path) -> ConfigLoadResult {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            return ConfigLoadResult {
                config: Config::default(),
                path: Some(path.to_path_buf()),
                error: Some(ConfigLoadError::Io(e)),
                explicit_path: false,
            };
        }
    };

    match toml::from_str(&content) {
        Ok(config) => ConfigLoadResult {
            config,
            path: Some(path.to_path_buf()),
            error: None,
            explicit_path: false,
        },
        Err(e) => ConfigLoadResult {
            config: Config::default(),
            path: Some(path.to_path_buf()),
            error: Some(ConfigLoadError::Parse(e)),
            explicit_path: false,
        },
    }
}

/// Loads configuration based on the provided options.
///
/// # Arguments
///
/// * `disabled` - If true, skip loading and return defaults
/// * `explicit_path` - If provided, use this path instead of the default
///
/// # Returns
///
/// A `ConfigLoadResult` containing the configuration and any errors encountered.
/// If `explicit_path` is provided and loading fails, the result will have
/// `explicit_path: true` to indicate that the error should be treated as fatal.
pub fn load_config(disabled: bool, explicit_path: Option<&Path>) -> ConfigLoadResult {
    if disabled {
        return ConfigLoadResult {
            config: Config::default(),
            path: None,
            error: None,
            explicit_path: false,
        };
    }

    let is_explicit = explicit_path.is_some();

    let path = match explicit_path {
        Some(p) => p.to_path_buf(),
        None => match default_config_path() {
            Some(p) => p,
            None => {
                // Can't determine config path; use defaults silently
                return ConfigLoadResult {
                    config: Config::default(),
                    path: None,
                    error: None,
                    explicit_path: false,
                };
            }
        },
    };

    // If using default path and file doesn't exist, silently use defaults
    if !is_explicit && !path.exists() {
        return ConfigLoadResult {
            config: Config::default(),
            path: Some(path),
            error: None,
            explicit_path: false,
        };
    }

    let mut result = load_from_path(&path);
    result.explicit_path = is_explicit;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.ui.syntax_highlighting.is_none());
        assert!(config.experimental.zsh_hooks.is_none());
        assert!(config.experimental.terminal_shell_integration.is_none());
    }

    #[test]
    fn test_full_config() {
        let toml = r#"
            [ui]
            syntax-highlighting = true

            [experimental]
            zsh-hooks = true
            terminal-shell-integration = false
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.ui.syntax_highlighting, Some(true));
        assert_eq!(config.experimental.zsh_hooks, Some(true));
        assert_eq!(config.experimental.terminal_shell_integration, Some(false));
    }

    #[test]
    fn test_partial_config() {
        let toml = r#"
            [ui]
            syntax-highlighting = false
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.ui.syntax_highlighting, Some(false));
        assert!(config.experimental.zsh_hooks.is_none());
    }

    #[test]
    fn test_unknown_fields_ignored() {
        let toml = r#"
            [ui]
            syntax-highlighting = true
            unknown-field = "should be ignored"
            another-unknown = 42

            [experimental]
            zsh-hooks = false
            future-feature = true

            [unknown-section]
            foo = "bar"
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.ui.syntax_highlighting, Some(true));
        assert_eq!(config.experimental.zsh_hooks, Some(false));
    }

    #[test]
    fn test_default_config_path() {
        // This test just verifies that default_config_path doesn't panic
        // and returns a reasonable path structure
        if let Some(path) = default_config_path() {
            assert!(path.ends_with("brush/config.toml"));
        }
    }

    #[test]
    fn test_load_config_disabled() {
        let result = load_config(true, None);
        assert!(result.path.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_load_config_nonexistent_default() {
        // When using default path and file doesn't exist, should return defaults without error
        let result = load_config(false, None);
        // We may or may not get a path depending on platform, but shouldn't error
        assert!(result.error.is_none());
    }

    #[test]
    fn test_load_config_nonexistent_explicit() {
        let path = Path::new("/nonexistent/path/to/config.toml");
        let result = load_config(false, Some(path));
        assert!(result.error.is_some());
        assert!(matches!(result.error, Some(ConfigLoadError::Io(_))));
    }

    fn make_test_args() -> CommandLineArgs {
        // Use clap parsing to get defaults - this won't break when fields are added
        CommandLineArgs::try_parse_from(["brush"]).unwrap()
    }

    #[test]
    fn test_to_ui_options_defaults_only() {
        let config = Config::default();
        let args = make_test_args();
        let ui = config.to_ui_options(&args, false);

        assert!(!ui.disable_bracketed_paste);
        assert!(!ui.disable_color);
        assert!(ui.disable_highlighting); // !enable_highlighting
        assert!(!ui.terminal_shell_integration);
        assert!(!ui.zsh_style_hooks);
    }

    #[test]
    fn test_to_ui_options_config_overrides_defaults() {
        let toml = r#"
            [ui]
            syntax-highlighting = true

            [experimental]
            zsh-hooks = true
            terminal-shell-integration = true
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let args = make_test_args();

        // CLI values match defaults, so config should take effect
        let ui = config.to_ui_options(&args, false);

        assert!(!ui.disable_highlighting); // config enabled highlighting
        assert!(ui.terminal_shell_integration);
        assert!(ui.zsh_style_hooks);
    }

    #[test]
    fn test_to_ui_options_cli_overrides_config() {
        let toml = r#"
            [ui]
            syntax-highlighting = true

            [experimental]
            zsh-hooks = true
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let mut args = make_test_args();
        args.enable_highlighting = true;
        args.zsh_style_hooks = true;

        // CLI explicitly enables highlighting and zsh-hooks (differs from default of false)
        let ui = config.to_ui_options(&args, false);

        assert!(!ui.disable_highlighting); // CLI enabled highlighting
        assert!(ui.zsh_style_hooks); // CLI enabled
    }

    #[test]
    fn test_to_ui_options_cli_only_settings() {
        let config = Config::default();
        let mut args = make_test_args();
        args.disable_bracketed_paste = true;
        args.disable_color = true;

        let ui = config.to_ui_options(&args, false);

        assert!(ui.disable_bracketed_paste);
        assert!(ui.disable_color);
    }
}
