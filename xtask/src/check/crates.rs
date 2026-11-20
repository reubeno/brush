//! Structural rules that every published workspace crate must satisfy: files it
//! must carry, metadata it must declare. Add a rule by writing a function with
//! the [`Rule`] signature and listing it in [`RULES`].

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use xshell::{Shell, cmd};

/// A published workspace crate, as seen by the rules.
pub struct Crate {
    /// The package name.
    pub name: String,
    /// The crate's directory (the one holding its `Cargo.toml`).
    pub dir: PathBuf,
}

/// Checks one crate, given the workspace root. Returns `Ok(Some(problem))` for
/// a violation; `Err` is reserved for failing to perform the check at all.
type Rule = fn(&Path, &Crate) -> Result<Option<String>>;

/// Every rule, with the name it is reported under.
const RULES: &[(&str, Rule)] = &[("LICENSE symlink", license_symlink)];

/// Runs every rule against every published crate, reporting all violations
/// before failing.
pub fn check(sh: &Shell) -> Result<()> {
    let (workspace_root, crates) = published_crates(sh)?;

    let mut violations = 0;
    for krate in crates {
        for (rule_name, rule) in RULES {
            if let Some(problem) = rule(&workspace_root, &krate)
                .with_context(|| format!("Failed to check {rule_name} for {}", krate.name))?
            {
                eprintln!("  {}: {rule_name}: {problem}", krate.name);
                violations += 1;
            }
        }
    }

    if violations > 0 {
        anyhow::bail!("{violations} crate rule violation(s) found");
    }
    Ok(())
}

/// Returns the workspace root and the workspace members that are published to
/// crates.io.
fn published_crates(sh: &Shell) -> Result<(PathBuf, Vec<Crate>)> {
    #[derive(serde::Deserialize)]
    struct Metadata {
        workspace_root: PathBuf,
        packages: Vec<Package>,
    }

    #[derive(serde::Deserialize)]
    struct Package {
        name: String,
        manifest_path: PathBuf,
        // `null` when publishable anywhere; `[]` for `publish = false`.
        publish: Option<Vec<String>>,
    }

    // `--no-deps` narrows the output to workspace members.
    let json = cmd!(sh, "cargo metadata --no-deps --format-version 1")
        .quiet()
        .read()
        .context("Failed to read cargo metadata")?;
    let metadata: Metadata =
        serde_json::from_str(&json).context("Failed to parse cargo metadata")?;

    let crates = metadata
        .packages
        .into_iter()
        .filter(|p| p.publish.as_ref().is_none_or(|r| !r.is_empty()))
        .filter_map(|p| {
            let dir = p.manifest_path.parent()?.to_path_buf();
            Some(Crate { name: p.name, dir })
        })
        .collect();

    Ok((metadata.workspace_root, crates))
}

/// The crate must carry a `LICENSE` that is a symlink to the workspace's
/// `LICENSE`, so `cargo package` ships the license text.
///
/// A no-op off Unix: Git for Windows checks symlinks out as plain files by
/// default, so the link can't be judged from the working tree there.
fn license_symlink(workspace_root: &Path, krate: &Crate) -> Result<Option<String>> {
    if cfg!(not(unix)) {
        return Ok(None);
    }

    let path = krate.dir.join("LICENSE");
    let shown = path.strip_prefix(workspace_root).unwrap_or(&path).display();

    let expected = workspace_root
        .join("LICENSE")
        .canonicalize()
        .context("Failed to find the workspace LICENSE")?;
    let ok = path.is_symlink() && path.canonicalize().is_ok_and(|p| p == expected);
    Ok((!ok).then(|| format!("{shown} must be a symlink to the workspace LICENSE")))
}
