//! Environment variable retrieval for Windows.
//!
//! On Windows, well-known environment variable names are normalized to their
//! canonical POSIX forms (e.g. `Path` → `PATH`), and `HOME` is synthesized
//! from `USERPROFILE` or `HOMEDRIVE`+`HOMEPATH` if not already present.

use std::collections::HashMap;

/// Retrieves environment variables from the host process, applying
/// Windows-specific fixups.
///
/// Normalizes well-known variable names to POSIX conventions, copies
/// `TEMP`/`TMP` to `TMPDIR` (preserving originals for native Windows apps),
/// and synthesizes `HOME` if it is not natively defined.
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    let mut vars = HashMap::new();

    // Normalize host env vars and inject them into a hash map.
    for (k, v) in std::env::vars() {
        let normalized = normalize_env_name(&k);
        vars.insert(normalized, v);
    }

    // Synthesize HOME from Windows-native variables if not already present.
    if !vars.contains_key("HOME") {
        let home = vars.get("USERPROFILE").cloned().or_else(|| {
            let d = vars.get("HOMEDRIVE")?;
            let p = vars.get("HOMEPATH")?;
            Some(format!("{d}{p}"))
        });
        if let Some(home) = home {
            vars.insert("HOME".to_string(), home);
        }
    }

    // Copy TEMP/TMP to TMPDIR if TMPDIR doesn't already exist.
    if !vars.contains_key("TMPDIR") {
        if let Some(tmp) = vars.get("TEMP").or_else(|| vars.get("TMP")).cloned() {
            vars.insert("TMPDIR".to_string(), tmp);
        }
    }

    vars.into_iter().collect::<Vec<_>>().into_iter()
}

/// Normalizes the case of well-known environment variable names to their
/// canonical POSIX forms (`Path` → `PATH`, `home` → `HOME`).
///
/// Variables like `TEMP`/`TMP` are not renamed here; they are copied to
/// `TMPDIR` by the caller so that both the Windows and POSIX names are present.
///
/// # Arguments
///
/// * `name` - The environment variable name to normalize.
fn normalize_env_name(name: &str) -> String {
    if name.eq_ignore_ascii_case("PATH") {
        "PATH".to_string()
    } else if name.eq_ignore_ascii_case("HOME") {
        "HOME".to_string()
    } else {
        name.to_string()
    }
}
