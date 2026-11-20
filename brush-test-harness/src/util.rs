//! Utility functions for the test harness.

use anyhow::Result;

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

/// Reads what a pty session read from the shell, from expectrl's log of the session.
#[cfg(pty)]
pub fn read_expectrl_log(log: Vec<u8>) -> Result<String> {
    use descape::UnescapeExt;

    // expectrl logs each chunk it reads as a string (escaped, as Rust's `Debug` does), if it's
    // UTF-8, or else as a list of bytes (as when a character is split between chunks); so the
    // chunks' bytes are put back together before they're decoded.
    let mut output = Vec::new();
    for line in String::from_utf8(log)?.lines() {
        if let Some(chunk) = line
            .strip_prefix("read: \"")
            .and_then(|line| line.strip_suffix('"'))
        {
            let chunk = chunk
                .to_unescaped()
                .map_err(|err| anyhow::anyhow!("invalid escape in pty log: {err:?}"))?;
            output.extend_from_slice(chunk.as_bytes());
        } else if let Some(bytes) = line
            .strip_prefix("read:(bytes): [")
            .and_then(|line| line.strip_suffix(']'))
        {
            for byte in bytes.split(", ") {
                output.push(byte.parse()?);
            }
        }
    }

    Ok(String::from_utf8_lossy(&output).into_owned())
}

/// Makes what a pty session read from the shell human-readable, by removing VT escape sequences.
pub fn make_expectrl_output_readable<S: AsRef<str>>(output: S) -> String {
    strip_ansi_escapes::strip_str(output)
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

/// Resolves the first element of the given launcher token list to an absolute path.
///
/// This is needed because the test harness clears env vars (including `PATH`)
/// before spawning child processes, so a launcher binary referenced by name
/// (e.g., `wasmtime`) would fail to resolve at exec time.
pub fn resolve_launcher_path(tokens: &mut [String]) -> Result<()> {
    let Some(first) = tokens.first() else {
        return Ok(());
    };
    if std::path::Path::new(first.as_str()).is_absolute() {
        return Ok(());
    }
    let resolved = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join(first.as_str()))
        .find(|p| p.is_file())
        .ok_or_else(|| anyhow::anyhow!("could not resolve launcher binary '{first}' in PATH"))?;
    tokens[0] = resolved.to_string_lossy().into_owned();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_output_keeps_backslashes() {
        // A transcript is decoded already, so a backslash in it is just text.
        let output = "PS1='test\\$ '\n";
        assert_eq!(make_expectrl_output_readable(output), output);
    }

    #[test]
    fn readable_output_drops_vt_escape_sequences() {
        assert_eq!(
            make_expectrl_output_readable("\u{1b}[31mred\u{1b}[0m"),
            "red"
        );
    }

    #[cfg(pty)]
    #[test]
    fn log_is_decoded_once() {
        // As expectrl logs what it reads: each chunk as a string, if it's UTF-8, or else as
        // bytes (as when a character is split between chunks).
        let log = format!(
            "write: {:?}\nread: {:?}\nread:(bytes): {:?}\nread:(bytes): {:?}\n",
            "echo",
            "a \\$ b\r\n",
            &[0xc3_u8][..],
            &[0xa9_u8, b'!'][..],
        );
        assert_eq!(
            read_expectrl_log(log.into_bytes()).unwrap(),
            "a \\$ b\r\né!"
        );
    }
}
