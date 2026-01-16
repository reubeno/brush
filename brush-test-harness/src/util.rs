//! Utility functions for the test harness.

use anyhow::Result;
use descape::UnescapeExt;

/// Get the OS ID from /etc/os-release file.
/// Returns the value of the ID field, which is the canonical OS identifier.
/// For example: "ubuntu", "opensuse-tumbleweed", "fedora", etc.
pub fn get_host_os_id() -> Option<String> {
    os_release::OsRelease::new().ok().and_then(|info| {
        if info.id.is_empty() {
            None
        } else {
            Some(info.id)
        }
    })
}

/// Reads and processes the expectrl log output.
#[cfg(unix)]
pub fn read_expectrl_log(log: Vec<u8>) -> Result<String> {
    let output_str = String::from_utf8(log)?;
    let output: String = output_str
        .lines()
        .filter(|line| line.starts_with("read:"))
        .map(|line| {
            line.strip_prefix("read: \"")
                .unwrap()
                .strip_suffix('"')
                .unwrap()
        })
        .collect();

    Ok(output)
}

/// Makes expectrl output human-readable by unescaping and stripping ANSI codes.
pub fn make_expectrl_output_readable<S: AsRef<str>>(output: S) -> String {
    // Unescape the escaping done by expectrl's logging mechanism.
    let unescaped = output.as_ref().to_unescaped().unwrap().to_string();

    // Remove VT escape sequences.
    strip_ansi_escapes::strip_str(unescaped)
}

/// Writes a diff between two strings to a writer.
pub fn write_diff(
    writer: &mut impl std::io::Write,
    indent: usize,
    left: &str,
    right: &str,
) -> Result<()> {
    use colored::Colorize;

    let indent_str = " ".repeat(indent);

    let diff = diff::lines(left, right);
    for d in diff {
        let formatted = match d {
            diff::Result::Left(l) => std::format!("{indent_str}- {l}").red(),
            diff::Result::Both(l, _) => std::format!("{indent_str}  {l}").bright_black(),
            diff::Result::Right(r) => std::format!("{indent_str}+ {r}").green(),
        };

        writeln!(writer, "{formatted}")?;
    }

    Ok(())
}

/// Gets the bash version string from the given bash path.
pub fn get_bash_version_str(bash_path: &std::path::Path) -> Result<String> {
    use anyhow::Context;

    let output = std::process::Command::new(bash_path)
        .arg("--norc")
        .arg("--noprofile")
        .arg("-c")
        .arg("echo -n ${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}.${BASH_VERSINFO[2]}")
        .output()
        .context("failed to retrieve bash version")?
        .stdout;

    let ver_str = String::from_utf8(output)?;

    Ok(ver_str)
}
