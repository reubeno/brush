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

    vars.into_iter()
}

/// Normalizes the case of well-known environment variable names to their
/// canonical POSIX forms (`Path` → `PATH`, `home` → `HOME`).
///
/// # Arguments
///
/// * `name` - The environment variable name to normalize.
fn normalize_env_name(name: &str) -> String {
    // Normalize well-known variable names so that later lookups by
    // canonical (uppercase) spelling always succeed regardless of the
    // host's original casing.
    const WELL_KNOWN: &[&str] = &[
        "PATH",
        "HOME",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "TEMP",
        "TMP",
        "TMPDIR",
    ];

    for &canonical in WELL_KNOWN {
        if name.eq_ignore_ascii_case(canonical) {
            return canonical.to_string();
        }
    }

    name.to_string()
}
