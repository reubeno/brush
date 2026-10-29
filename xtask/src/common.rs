//! Common utilities shared across xtask commands.
//!
//! This module provides shared functionality for:
//! - Build profile selection (debug vs release)
//! - Workspace root and target directory discovery
//! - Brush binary location for test commands

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::ValueEnum;
use xshell::{Shell, cmd};

/// Build profile for selecting which binary to use.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum BuildProfile {
    /// Debug build (target/debug/).
    #[default]
    Debug,
    /// Release build (target/release/).
    Release,
}

impl BuildProfile {
    /// Returns the target subdirectory name for this profile.
    #[cfg(unix)]
    #[must_use]
    pub const fn target_dir_name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

/// Find the workspace root directory.
///
/// This walks up from the xtask crate directory to find the workspace root
/// (the directory containing the top-level Cargo.toml with [workspace]).
#[cfg(unix)]
pub fn find_workspace_root() -> Result<PathBuf> {
    // Start from the xtask crate directory
    let xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // The workspace root is the parent of xtask/
    let workspace_root = xtask_dir
        .parent()
        .context("Failed to find workspace root (parent of xtask)")?;

    Ok(workspace_root.to_path_buf())
}

/// The parts of `cargo metadata` that xtask reads.
#[derive(serde::Deserialize)]
struct Metadata {
    target_directory: PathBuf,
    packages: Vec<Package>,
}

#[derive(serde::Deserialize)]
struct Package {
    name: String,
    version: String,
}

fn cargo_metadata() -> Result<Metadata> {
    let sh = Shell::new()?;
    // `--no-deps` skips resolving the dependency graph.
    let json = cmd!(sh, "cargo metadata --no-deps --format-version 1")
        .quiet()
        .read()
        .context("Failed to read cargo metadata")?;
    serde_json::from_str(&json).context("Failed to parse cargo metadata")
}

/// Find the directory cargo builds into, wherever `CARGO_TARGET_DIR`,
/// `build.target-dir` or the default puts it.
pub fn find_target_dir() -> Result<PathBuf> {
    Ok(cargo_metadata()?.target_directory)
}

/// Find the version of the workspace's package `name`.
pub fn find_package_version(name: &str) -> Result<String> {
    cargo_metadata()?
        .packages
        .into_iter()
        .find(|package| package.name == name)
        .map(|package| package.version)
        .with_context(|| format!("No package named {name} in the workspace"))
}

/// Find the brush binary path for the given build profile.
///
/// If `override_path` is provided, it is used directly (after validation).
/// Otherwise, the binary is located in the workspace's target directory
/// based on the specified profile.
#[cfg(unix)]
pub fn find_brush_binary(
    override_path: Option<&PathBuf>,
    profile: BuildProfile,
) -> Result<PathBuf> {
    let binary_path = if let Some(path) = override_path {
        path.clone()
    } else {
        let binary_name = if cfg!(windows) { "brush.exe" } else { "brush" };
        find_target_dir()?
            .join(profile.target_dir_name())
            .join(binary_name)
    };

    // Canonicalize to get absolute path and verify existence
    let canonical_path = binary_path.canonicalize().with_context(|| {
        format!(
            "Brush binary not found at: {} (profile: {:?}). Did you run `cargo build{}`?",
            binary_path.display(),
            profile,
            if profile == BuildProfile::Release {
                " --release"
            } else {
                ""
            }
        )
    })?;

    Ok(canonical_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn test_profile_dir_names() {
        assert_eq!(BuildProfile::Debug.target_dir_name(), "debug");
        assert_eq!(BuildProfile::Release.target_dir_name(), "release");
    }

    #[cfg(unix)]
    #[test]
    fn test_find_workspace_root() {
        let root = find_workspace_root();
        assert!(root.is_ok(), "Should find workspace root");
        let root = root.unwrap();
        // The workspace root should contain Cargo.toml
        assert!(root.join("Cargo.toml").exists());
        // And it should contain the xtask directory
        assert!(root.join("xtask").exists());
    }

    #[test]
    fn test_find_target_dir() {
        // Wherever cargo was told to build, this test was built there. (The
        // running executable's path has its symlinks resolved.)
        let exe = std::env::current_exe().unwrap().canonicalize().unwrap();
        assert!(exe.starts_with(find_target_dir().unwrap().canonicalize().unwrap()));
    }

    #[test]
    fn test_find_package_version() {
        assert_eq!(
            find_package_version("xtask").unwrap(),
            env!("CARGO_PKG_VERSION")
        );
        assert!(find_package_version("no-such-package").is_err());
    }
}
