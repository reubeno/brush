//! Integration tests for the `CommandInterceptor` capability-confinement hooks
//! on `ShellExtensions` (`before_exec` / `before_open`).
//!
//! These tests prove that an embedding host can deny external command execution
//! and file opens *in-process*, including for commands whose name contains a
//! path separator (e.g. `/bin/rm`). That path-separator branch historically
//! bypassed both the PATH search and the builtin table, so confining it is the
//! whole point of `before_exec`.
//!
//! These tests target unix. Denied commands use `/bin/rm` — its existence is
//! irrelevant because `before_exec` denies it *before* any spawn. Commands that
//! are actually executed use `/usr/bin/true`, which exists on both Linux and
//! macOS (note: `/bin/true` is absent on macOS).
#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use brush_core::extensions::{
    CommandInterceptor, ErrorFormatter, ExecDecision, OpenDecision, ShellExtensions,
};

/// An interceptor that denies any external program whose basename is in a deny
/// list, and denies write-opens of any path that is not under `allowed_write_dir`.
/// All decisions are recorded so tests can assert the hook actually fired.
#[derive(Clone, Default)]
struct PolicyInterceptor {
    denied_basenames: Arc<Vec<String>>,
    allowed_write_dir: Arc<Mutex<Option<PathBuf>>>,
    exec_calls: Arc<Mutex<Vec<String>>>,
    open_calls: Arc<Mutex<Vec<(PathBuf, bool)>>>,
}

impl PolicyInterceptor {
    fn new(denied_basenames: &[&str], allowed_write_dir: Option<PathBuf>) -> Self {
        Self {
            denied_basenames: Arc::new(denied_basenames.iter().map(|s| (*s).to_string()).collect()),
            allowed_write_dir: Arc::new(Mutex::new(allowed_write_dir)),
            exec_calls: Arc::new(Mutex::new(Vec::new())),
            open_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn exec_calls(&self) -> Vec<String> {
        self.exec_calls.lock().unwrap().clone()
    }

    fn open_calls(&self) -> Vec<(PathBuf, bool)> {
        self.open_calls.lock().unwrap().clone()
    }
}

impl CommandInterceptor for PolicyInterceptor {
    fn before_exec(&self, program: &str, _args: &[String]) -> ExecDecision {
        self.exec_calls.lock().unwrap().push(program.to_string());

        // Match on the basename so that `rm` and `/bin/rm` are both caught.
        let basename = Path::new(program)
            .file_name()
            .map_or_else(|| program.to_string(), |s| s.to_string_lossy().into_owned());

        if self.denied_basenames.iter().any(|d| d == &basename) {
            ExecDecision::Deny(format!("'{basename}' is not permitted by policy"))
        } else {
            ExecDecision::Allow
        }
    }

    fn before_open(&self, path: &Path, write: bool) -> OpenDecision {
        self.open_calls
            .lock()
            .unwrap()
            .push((path.to_path_buf(), write));

        if !write {
            return OpenDecision::Allow;
        }

        match self.allowed_write_dir.lock().unwrap().as_ref() {
            Some(dir) if path.starts_with(dir) => OpenDecision::Allow,
            Some(_) => OpenDecision::Deny(format!(
                "writes to {} are outside the permitted directory",
                path.display()
            )),
            None => OpenDecision::Allow,
        }
    }
}

/// Wire `PolicyInterceptor` into a `ShellExtensions` bundle that otherwise uses
/// brush's default behaviors.
#[derive(Clone, Default)]
struct PolicyExtensions;

impl ShellExtensions for PolicyExtensions {
    type ErrorFormatter = DefaultFormatter;
    type CommandInterceptor = PolicyInterceptor;
}

#[derive(Clone, Default)]
struct DefaultFormatter;
impl ErrorFormatter for DefaultFormatter {}

/// Builds a shell that uses the provided interceptor, skipping profile/rc and
/// environment inheritance so the test is hermetic, but with the default
/// builtin table registered (so `echo`, `.`/`source`, etc. behave normally).
async fn shell_with_interceptor(
    interceptor: PolicyInterceptor,
) -> Result<brush_core::Shell<PolicyExtensions>> {
    let builtins =
        brush_builtins::default_builtins::<PolicyExtensions>(brush_builtins::BuiltinSet::BashMode);

    let mut shell = brush_core::Shell::builder_with_extensions::<PolicyExtensions>()
        .command_interceptor(interceptor)
        .builtins(builtins)
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .build()
        .await?;

    // Provide a deterministic PATH so bare-name external commands can resolve.
    run(&mut shell, "export PATH=/bin:/usr/bin").await?;

    Ok(shell)
}

async fn run(shell: &mut brush_core::Shell<PolicyExtensions>, cmd: &str) -> Result<u8> {
    let params = shell.default_exec_params();
    let result = shell
        .run_string(cmd, &brush_core::SourceInfo::default(), &params)
        .await?;
    Ok(u8::from(result.exit_code))
}

/// `before_exec` must deny a command referenced by bare name (resolved via PATH).
#[tokio::test]
async fn denies_bare_name_command() -> Result<()> {
    let interceptor = PolicyInterceptor::new(&["rm"], None);
    let mut shell = shell_with_interceptor(interceptor.clone()).await?;

    let code = run(&mut shell, "rm /tmp/does-not-matter").await?;
    assert_ne!(code, 0, "denied `rm` must report a non-zero exit code");

    let exec_calls = interceptor.exec_calls();
    assert!(
        exec_calls.iter().any(|p| p.ends_with("rm")),
        "before_exec should have been consulted for `rm`; saw: {exec_calls:?}"
    );
    Ok(())
}

/// The load-bearing test: a command containing a path separator (`/bin/rm`)
/// historically bypassed PATH and the builtin table. `before_exec` must still
/// fire for it, proving the bypass is closed.
#[tokio::test]
async fn denies_absolute_path_command_closing_path_separator_bypass() -> Result<()> {
    let interceptor = PolicyInterceptor::new(&["rm"], None);
    let mut shell = shell_with_interceptor(interceptor.clone()).await?;

    let code = run(&mut shell, "/bin/rm /tmp/does-not-matter").await?;
    assert_ne!(
        code, 0,
        "denied `/bin/rm` (path-separator branch) must report a non-zero exit code"
    );

    let exec_calls = interceptor.exec_calls();
    assert!(
        exec_calls.iter().any(|p| p == "/bin/rm"),
        "before_exec must be consulted for the path-separator command `/bin/rm`; saw: {exec_calls:?}"
    );
    Ok(())
}

/// A permitted command must still run normally — the default decision is Allow.
#[tokio::test]
async fn allows_permitted_command() -> Result<()> {
    let interceptor = PolicyInterceptor::new(&["rm"], None);
    let mut shell = shell_with_interceptor(interceptor.clone()).await?;

    // `/usr/bin/true` is a path-separator command too, so this also proves the
    // Allow path of the path-separator branch. We use `/usr/bin/true` rather than
    // `/bin/true` because the latter does not exist on macOS (where `true` lives
    // only at `/usr/bin/true`); `/usr/bin/true` is present on both Linux and macOS.
    let code = run(&mut shell, "/usr/bin/true").await?;
    assert_eq!(code, 0, "permitted `/usr/bin/true` should succeed");

    let exec_calls = interceptor.exec_calls();
    assert!(
        exec_calls.iter().any(|p| p == "/usr/bin/true"),
        "before_exec should have observed `/usr/bin/true`; saw: {exec_calls:?}"
    );
    Ok(())
}

/// `before_open` must deny an output redirection that writes outside the
/// permitted directory, while allowing one inside it.
#[tokio::test]
async fn denies_write_outside_allowed_dir() -> Result<()> {
    let allowed = tempfile::tempdir()?;
    let forbidden = tempfile::tempdir()?;

    let interceptor = PolicyInterceptor::new(&[], Some(allowed.path().to_path_buf()));
    let mut shell = shell_with_interceptor(interceptor.clone()).await?;

    // Writing inside the allowed dir is permitted.
    let allowed_file = allowed.path().join("ok.txt");
    let code = run(&mut shell, &format!("echo hi > {}", allowed_file.display())).await?;
    assert_eq!(code, 0, "write inside the allowed dir should succeed");
    assert!(
        allowed_file.exists(),
        "the permitted file should have been created"
    );

    // Writing outside the allowed dir is denied.
    let forbidden_file = forbidden.path().join("nope.txt");
    let code = run(
        &mut shell,
        &format!("echo hi > {}", forbidden_file.display()),
    )
    .await?;
    assert_ne!(code, 0, "write outside the allowed dir must fail");
    assert!(
        !forbidden_file.exists(),
        "the forbidden file must NOT have been created"
    );

    let open_calls = interceptor.open_calls();
    assert!(
        open_calls.iter().any(|(p, w)| *w && p == &forbidden_file),
        "before_open should have been consulted (with write=true) for the forbidden path; saw: {open_calls:?}"
    );
    Ok(())
}

/// Reads must be allowed by this policy (write=false), proving the `write` flag
/// is threaded correctly and read-only opens aren't accidentally denied.
#[tokio::test]
async fn allows_read_open() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let script = dir.path().join("snippet.sh");
    std::fs::write(&script, "X=hello\n")?;

    // allowed_write_dir is set to a *different* dir, so if reads were
    // misclassified as writes they would be denied.
    let other = tempfile::tempdir()?;
    let interceptor = PolicyInterceptor::new(&[], Some(other.path().to_path_buf()));
    let mut shell = shell_with_interceptor(interceptor.clone()).await?;

    let code = run(&mut shell, &format!(". {}", script.display())).await?;
    assert_eq!(code, 0, "sourcing (read-only open) should be permitted");

    let open_calls = interceptor.open_calls();
    assert!(
        open_calls.iter().any(|(p, w)| !*w && p == &script),
        "before_open should have observed a read-only open of the sourced file; saw: {open_calls:?}"
    );
    Ok(())
}
