//! Integration tests for the opt-in `sync_process_cwd` option: that the real
//! OS-level process working directory (`/proc/<pid>/cwd` on Linux) tracks the
//! shell's own logical working directory after `cd`, and that subshells never
//! touch it.
//!
//! All assertions live in one `#[test]` function. `std::env::set_current_dir`
//! mutates process-global state shared by every thread in this test binary;
//! spreading the checks across multiple `#[test]` functions (which `cargo
//! test` runs concurrently by default within one process) would race them
//! against each other. A dedicated file keeps this isolated from every other
//! test file, each of which is its own process.

#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn, clippy::expect_used)]

use anyhow::Result;

/// With the option off (the default), `cd` updates the shell's own working
/// directory but never touches the real process cwd. This is the behavior
/// every existing consumer (including brush-core's other tests, which
/// routinely call `set_working_dir` against temp dirs) depends on.
///
/// With the option on, the *root* shell's `cd` syncs the real process cwd,
/// but a subshell's `cd` (e.g. inside `$(...)`) must not: subshells run as
/// concurrent in-process clones sharing one OS process, and any of them
/// mutating process-global state would race with, and corrupt, every other
/// clone's own view of "its" directory.
#[tokio::test]
async fn sync_process_cwd_option() -> Result<()> {
    let original_cwd = std::env::current_dir()?;
    let target_dir = tempfile::tempdir()?;
    let target_path = target_dir.path().canonicalize()?;

    // Restores the real process cwd no matter how the test exits, so a
    // failure here doesn't leak a changed cwd into whatever runs next in
    // this process.
    struct RestoreCwd(std::path::PathBuf);
    impl Drop for RestoreCwd {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }
    let _restore = RestoreCwd(original_cwd.clone());

    // 1. Option off (default): `cd` must not touch the real process cwd.
    let mut shell = brush_core::Shell::builder()
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .build()
        .await?;
    assert!(
        !shell.options().sync_process_cwd,
        "sync_process_cwd must default to disabled"
    );
    shell.set_working_dir(&target_path)?;
    assert_eq!(
        shell.working_dir(),
        target_path,
        "the shell's own logical working dir must still update"
    );
    assert_eq!(
        std::env::current_dir()?,
        original_cwd,
        "with the option off, the real process cwd must be untouched"
    );

    // 2. Option on, root shell: `cd` must sync the real process cwd.
    let mut shell = brush_core::Shell::builder()
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .sync_process_cwd(true)
        .build()
        .await?;
    assert!(
        !shell.is_subshell(),
        "a freshly built shell must not be a subshell"
    );
    shell.set_working_dir(&target_path)?;
    assert_eq!(
        std::env::current_dir()?,
        target_path,
        "with the option on, the root shell's cd must sync the real process cwd"
    );

    // 3. Option on, subshell (as used for command substitution, background
    //    jobs, `(...)`, and function calls): `cd` must update the subshell's
    //    own logical working dir without ever touching the real process cwd,
    //    even though the option is enabled and the root shell already synced
    //    it to `target_path` above.
    let mut subshell = shell.clone();
    assert!(
        subshell.is_subshell(),
        "Shell::clone() must produce a subshell"
    );
    let other_dir = tempfile::tempdir()?;
    let other_path = other_dir.path().canonicalize()?;
    subshell.set_working_dir(&other_path)?;
    assert_eq!(
        subshell.working_dir(),
        other_path,
        "the subshell's own logical working dir must still update"
    );
    assert_eq!(
        std::env::current_dir()?,
        target_path,
        "a subshell's cd must never touch the real process cwd, even with the option on"
    );
    // The root shell's own logical working dir must be unaffected by the
    // subshell's `cd`, exactly as it is in bash today.
    assert_eq!(
        shell.working_dir(),
        target_path,
        "the root shell's own logical working dir must be unaffected by its subshell's cd"
    );

    Ok(())
}
