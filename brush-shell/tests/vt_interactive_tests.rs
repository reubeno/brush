//! Syntax highlighting options on the VT backend, over a real pseudo-terminal.
//!
//! Like the other pty tests, these run from a cargo target directory rather than
//! as prebuilt compatibility-test artifacts.

#![cfg(test)]
#![cfg(vt_backend)]
#![cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
#![allow(clippy::panic_in_result_fn)]

use std::time::Duration;

use anyhow::Context;
use expectrl::Expect;

#[test]
fn vt_highlighting_respects_cli_and_config() -> anyhow::Result<()> {
    for (config_enabled, flags, expected) in [
        (Some(false), vec![], false),
        (None, vec!["--enable-highlighting"], true),
        (Some(true), vec![], true),
        (Some(true), vec!["--disable-color"], false),
        (
            None,
            vec!["--enable-highlighting", "--disable-color"],
            false,
        ),
    ] {
        let config_dir = tempfile::tempdir()?;
        let config_path = config_dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            config_enabled.map_or_else(
                || "[ui]\n".to_owned(),
                |enabled| format!("[ui]\nsyntax-highlighting = {enabled}\n"),
            ),
        )?;
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("brush"));
        cmd.args(["--norc", "--noprofile", "--input-backend=vt", "--config"])
            .arg(&config_path)
            .args(&flags)
            .env("PS1", "brush> ")
            .env("TERM", "xterm-256color");
        let mut session = expectrl::Session::spawn(cmd)?;
        session.set_expect_timeout(Some(Duration::from_secs(8)));
        session.expect("brush> ").context("VT prompt missing")?;
        // Interrupt after typing so the captured bytes contain the redraws but
        // no command output that could supply its own color sequences.
        session.send("echo 'color' $HOME")?;
        session.send("\x03")?;
        let captured = session.expect("^C")?;
        let output = String::from_utf8_lossy(captured.before());
        assert_eq!(
            output.contains("\x1b[33m"),
            expected,
            "config={config_enabled:?}, flags={flags:?}: {output:?}"
        );
        session.expect("brush> ").context("VT prompt missing")?;
        session.send("\x04")?;
        session.expect(expectrl::Eof)?;
    }
    Ok(())
}
