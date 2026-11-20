//! Helpers for the tests that run brush at a pty.

#![allow(dead_code, reason = "each test binary uses only some of these")]

/// Clears `cmd`'s environment, so the user's can't change what's tested, then gives it just
/// what a shell needs: a `PATH` to find commands, and a `HOME` of its own, so nothing the
/// shell writes there (its history, for one) reaches the user's. Callers set `TERM` (and
/// `PS1`) themselves, since they test different terminals.
///
/// The `HOME` is removed when the returned directory is dropped, so keep it until the shell
/// has exited, e.g. in a [`WithHome`].
pub fn clean_env(cmd: &mut std::process::Command) -> std::io::Result<tempfile::TempDir> {
    let home = tempfile::tempdir()?;

    cmd.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        cmd.env("PATH", path);
    }
    cmd.env("HOME", home.path());
    Ok(home)
}

/// A command that runs brush in a clean environment (see [`clean_env`]), along with the
/// `HOME` made for it.
pub fn brush_command() -> std::io::Result<(std::process::Command, tempfile::TempDir)> {
    let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("brush"));
    let home = clean_env(&mut cmd)?;
    Ok((cmd, home))
}

/// A shell's session along with the `HOME` made for the shell, which is removed when this is
/// dropped. It derefs to the session.
pub struct WithHome<S> {
    session: S,
    _home: tempfile::TempDir,
}

impl<S> WithHome<S> {
    pub const fn new(session: S, home: tempfile::TempDir) -> Self {
        Self {
            session,
            _home: home,
        }
    }
}

impl<S> std::ops::Deref for WithHome<S> {
    type Target = S;

    fn deref(&self) -> &S {
        &self.session
    }
}

impl<S> std::ops::DerefMut for WithHome<S> {
    fn deref_mut(&mut self) -> &mut S {
        &mut self.session
    }
}
