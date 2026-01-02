//! Common utilities shared across xtask commands.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::ValueEnum;

/// Build profile for selecting which binary to use.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum BuildProfile {
    /// Debug build (target/debug/).
    Debug,
    /// Release build (target/release/).
    #[default]
    Release,
}

impl BuildProfile {
    /// Returns the target subdirectory name for this profile.
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
pub fn find_workspace_root() -> Result<PathBuf> {
    // Start from the xtask crate directory
    let xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // The workspace root is the parent of xtask/
    let workspace_root = xtask_dir
        .parent()
        .context("Failed to find workspace root (parent of xtask)")?;

    Ok(workspace_root.to_path_buf())
}

/// Find the brush binary path for the given build profile.
///
/// If `override_path` is provided, it is used directly (after validation).
/// Otherwise, the binary is located in the workspace's target directory
/// based on the specified profile.
pub fn find_brush_binary(
    override_path: Option<&PathBuf>,
    profile: BuildProfile,
) -> Result<PathBuf> {
    let binary_path = if let Some(path) = override_path {
        path.clone()
    } else {
        let workspace_root = find_workspace_root()?;
        let binary_name = if cfg!(windows) { "brush.exe" } else { "brush" };
        workspace_root
            .join("target")
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

    #[test]
    fn test_profile_dir_names() {
        assert_eq!(BuildProfile::Debug.target_dir_name(), "debug");
        assert_eq!(BuildProfile::Release.target_dir_name(), "release");
    }

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
}
