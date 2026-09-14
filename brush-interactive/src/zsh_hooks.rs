//! A bash-native adaptation of zsh's `precmd`/`preexec` hooks.
//!
//! zsh defines the hooks; [bash-preexec] is the adaptation of them to bash that the ecosystem
//! actually builds on, so it is what settles the details zsh leaves open for a bash shell.
//! brush implements the hooks in the shell itself rather than in shell script, and
//! interoperates with bash-preexec only as far as an interlock needs to: it claims
//! bash-preexec's inclusion guards, so a copy sourced later stands down instead of layering
//! its `DEBUG`-trap emulation on top of the real thing.
//!
//! Setup ([`init`]) runs once, before the shell loads its profile and rc files; dispatch
//! ([`run_precmd`] and [`run_preexec`]) runs from the interactive loop. The contract as a
//! whole is documented in `docs/reference/zsh-hooks.md`.
//!
//! [bash-preexec]: https://github.com/rcaloras/bash-preexec

use std::ops::ControlFlow;

use brush_core::variables::{ArrayLiteral, ShellValueLiteral};

use crate::InteractiveOptions;
use crate::ShellError;

/// The registry naming the hooks that run before each prompt.
const PRECMD_FUNCTIONS: &str = "precmd_functions";

/// The registry naming the hooks that run before each command line.
const PREEXEC_FUNCTIONS: &str = "preexec_functions";

/// The bash-preexec inclusion guards, current and legacy. Sourcing bash-preexec is a no-op
/// while either holds a non-empty value, so brush claims both.
const BASH_PREEXEC_GUARDS: [&str; 2] = ["bash_preexec_imported", "__bp_imported"];

/// Prepares a shell for zsh-style `precmd`/`preexec` hooks.
///
/// Seeds the hook registries and claims the [bash-preexec] inclusion guards, so a copy sourced
/// later stands down and leaves the hooks to us. Documented in `docs/reference/zsh-hooks.md`.
///
/// Call before profile and rc files load, so they observe the state -- and only for a shell
/// that will go on to read commands interactively, since only such a shell ever reaches
/// dispatch and so only such a shell may claim the contract. Whether the hooks are wanted at
/// all is decided here, by the same predicate dispatch uses, so the two cannot disagree.
///
/// A rejected assignment is fatal: a contract claimed only partway advertises hooks brush
/// never registered.
///
/// [bash-preexec]: https://github.com/rcaloras/bash-preexec
///
/// # Arguments
///
/// * `shell` - The shell to prepare.
/// * `options` - The options the interactive loop will run with.
pub fn init<SE: brush_core::ShellExtensions>(
    shell: &mut brush_core::Shell<SE>,
    options: &InteractiveOptions,
) -> Result<(), ShellError> {
    if !enabled(shell, options) {
        return Ok(());
    }

    // No export attribute is added, so a child bash still gets the real thing.
    for guard_name in BASH_PREEXEC_GUARDS {
        assign_global(
            shell,
            guard_name,
            ShellValueLiteral::Scalar("defined".into()),
        )?;
    }

    // Seeded with the bare hook names, as bash-preexec seeds them -- but by assignment, not
    // `+=`, because this runs before rc files rather than after them.
    for (array_name, hook_name) in [(PRECMD_FUNCTIONS, "precmd"), (PREEXEC_FUNCTIONS, "preexec")] {
        let value = ShellValueLiteral::Array(ArrayLiteral(vec![(None, hook_name.into())]));
        assign_global(shell, array_name, value)?;
    }

    Ok(())
}

/// Runs the `precmd` hooks. Breaks with the result of one that exited the shell.
///
/// # Arguments
///
/// * `shell` - The shell to run the hooks in.
/// * `options` - The options the interactive loop is running with.
pub(crate) async fn run_precmd<SE: brush_core::ShellExtensions>(
    shell: &mut brush_core::Shell<SE>,
    options: &InteractiveOptions,
) -> Result<ControlFlow<brush_core::ExecutionResult>, ShellError> {
    let Some(hook_names) = pending_hooks(shell, options, PRECMD_FUNCTIONS) else {
        return Ok(ControlFlow::Continue(()));
    };

    dispatch(shell, PRECMD_FUNCTIONS, hook_names, None).await
}

/// Runs the `preexec` hooks for a line the user entered. Breaks with the result of one that
/// exited the shell.
///
/// # Arguments
///
/// * `shell` - The shell to run the hooks in.
/// * `options` - The options the interactive loop is running with.
/// * `command_line` - The line as entered by the user; passed to each hook as `$1`.
pub(crate) async fn run_preexec<SE: brush_core::ShellExtensions>(
    shell: &mut brush_core::Shell<SE>,
    options: &InteractiveOptions,
    command_line: &str,
) -> Result<ControlFlow<brush_core::ExecutionResult>, ShellError> {
    // The registry is consulted ahead of `line_runs_a_command`, which parses: with no hook to
    // run there is nothing to decide, and every typed line would pay for the answer.
    let Some(hook_names) = pending_hooks(shell, options, PREEXEC_FUNCTIONS) else {
        return Ok(ControlFlow::Continue(()));
    };

    if !line_runs_a_command(shell, command_line) {
        return Ok(ControlFlow::Continue(()));
    }

    dispatch(
        shell,
        PREEXEC_FUNCTIONS,
        hook_names,
        Some(command_line.trim_end_matches('\n')),
    )
    .await
}

/// Whether zsh-style hooks are live for this shell: enabled by the options, and interactive
/// in the `$-` sense (`script | brush -s` reads commands through the interactive loop but
/// isn't). Setup and dispatch share this so they can't disagree.
fn enabled<SE: brush_core::ShellExtensions>(
    shell: &brush_core::Shell<SE>,
    options: &InteractiveOptions,
) -> bool {
    options.zsh_style_hooks && shell.options().interactive
}

/// The entries `array_var_name` names, read once as `"${name[@]}"` would expand it -- or
/// `None` when a dispatch would run nothing, so the caller can skip the rest of its work.
/// The registries are seeded non-empty, so an entry naming a `precmd`/`preexec` nobody
/// defined is the ordinary case, not an unusual one.
fn pending_hooks<SE: brush_core::ShellExtensions>(
    shell: &brush_core::Shell<SE>,
    options: &InteractiveOptions,
    array_var_name: &str,
) -> Option<Vec<String>> {
    if !enabled(shell, options) {
        return None;
    }

    // Reads a shell variable as the list of words `"${name[@]}"` would expand to. An unset
    // variable yields nothing; a scalar yields itself, since a scalar *is* element 0 in bash.
    let hook_names = shell
        .env_var(array_var_name)
        .map(|var| var.value().element_values(shell))
        .unwrap_or_default();

    hook_names
        .iter()
        .any(|name| shell.funcs().get(name).is_some())
        .then_some(hook_names)
}

/// Whether the line runs any command: blank, comment-only, and unparseable lines run none,
/// and so dispatch no `preexec` -- matching the `DEBUG` trap bash-preexec dispatches from.
/// Parsing is what answers this; a text test for "blank or all comment" would still miss the
/// syntax error. So a shell with a `preexec` hook registered pays a parse of the line here and
/// another in `run_string` moments later. (`parse_string` is cached, which often makes the
/// second one free, but it's a bounded cache and nothing here should lean on a hit.)
fn line_runs_a_command<SE: brush_core::ShellExtensions>(
    shell: &brush_core::Shell<SE>,
    line: &str,
) -> bool {
    shell
        .parse_string(line)
        .is_ok_and(|program| !program.complete_commands.is_empty())
}

/// Invokes the named hooks. Each sees what the last command left in `$?`, `PIPESTATUS`, and
/// `$_`, restored before every hook and again afterwards. Breaks with the result of a hook
/// that exited the shell. The full contract is in `docs/reference/zsh-hooks.md`.
///
/// # Arguments
///
/// * `shell` - The shell to run the hooks in.
/// * `array_var_name` - The registry the names came from; used only in diagnostics.
/// * `hook_names` - The registry's entries, already read.
/// * `arg` - Optional argument to pass to each hook.
async fn dispatch<SE: brush_core::ShellExtensions>(
    shell: &mut brush_core::Shell<SE>,
    array_var_name: &str,
    hook_names: Vec<String>,
    arg: Option<&str>,
) -> Result<ControlFlow<brush_core::ExecutionResult>, ShellError> {
    let saved_status = shell.save_command_status();

    for hook_name in hook_names {
        // Asked here rather than inferred from a `FunctionNotFound` error, which a hook body
        // could raise on its own. Asked per entry, not once up front, so a hook that defines
        // a later one in the same dispatch is honored -- as it is under bash-preexec, whose
        // `type -t` guard also sits inside the loop.
        if shell.funcs().get(&hook_name).is_none() {
            tracing::debug!("{array_var_name}: skipping '{hook_name}'; not a function");
            continue;
        }

        // Each hook sees the same status, whatever an earlier hook ran.
        shell.restore_command_status(saved_status.clone());

        // A failing hook is reported and the rest still run. (PROMPT_COMMAND differs: an
        // error that escapes `run_string` there ends the interactive loop.)
        match shell
            .invoke_function(&hook_name, arg, shell.default_exec_params())
            .await
        {
            // Returned without restoring: the shell is exiting, and `$?` belongs to the hook
            // that exited it.
            Ok(result) if result.is_exit() => return Ok(ControlFlow::Break(result)),
            Ok(_) => {}
            Err(e) => {
                let mut stderr = shell.stderr();
                let _ = shell.display_error(&mut stderr, &e);
            }
        }
    }

    shell.restore_command_status(saved_status);

    Ok(ControlFlow::Continue(()))
}

/// Assigns to a global variable as `name=value` would: an existing variable keeps its
/// attributes, and a readonly one rejects the assignment with an error.
fn assign_global<SE: brush_core::ShellExtensions>(
    shell: &mut brush_core::Shell<SE>,
    name: &str,
    value: ShellValueLiteral,
) -> Result<(), brush_core::Error> {
    shell.env_mut().update_or_add(
        name,
        value,
        |_| Ok(()),
        brush_core::env::EnvironmentLookup::Anywhere,
        brush_core::env::EnvironmentScope::Global,
    )
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use super::*;

    /// Builds a shell that zsh-style hooks would dispatch for, with `guard_readonly` marked
    /// readonly beforehand so the assignments in [`init`] must fail.
    async fn shell_with_readonly(
        interactive: bool,
        guard_readonly: &str,
    ) -> Result<brush_core::Shell<brush_core::extensions::DefaultShellExtensions>, ShellError> {
        let mut shell = brush_core::Shell::builder()
            .interactive(interactive)
            .profile(brush_core::ProfileLoadBehavior::Skip)
            .rc(brush_core::RcLoadBehavior::Skip)
            .build()
            .await?;

        let mut var = brush_core::ShellVariable::new("inherited");
        var.set_readonly();
        shell.env_mut().set_global(guard_readonly, var)?;

        Ok(shell)
    }

    fn hooks_enabled() -> InteractiveOptions {
        InteractiveOptions {
            zsh_style_hooks: true,
            ..Default::default()
        }
    }

    /// Claiming the contract only partway would advertise hooks brush never registered, so
    /// all four assignments are load-bearing and a rejection is fatal. Unit-tested because
    /// only an embedder assigning before us can reach this state. The error kind is pinned
    /// too: a bare `is_err()` would also pass on a panic or an unrelated failure.
    #[tokio::test]
    async fn init_fails_if_any_state_is_readonly() -> Result<(), ShellError> {
        for name in [
            "bash_preexec_imported",
            "__bp_imported",
            PRECMD_FUNCTIONS,
            PREEXEC_FUNCTIONS,
        ] {
            let mut shell = shell_with_readonly(true, name).await?;
            let result = init(&mut shell, &hooks_enabled());
            assert!(
                matches!(&result, Err(ShellError::ShellError(e))
                    if matches!(e.kind(), brush_core::ErrorKind::ReadonlyVariable)),
                "readonly '{name}' should have failed initialization as a readonly \
                 violation; got: {result:?}"
            );
        }

        Ok(())
    }

    /// A shell the hooks won't dispatch for is left completely alone -- the readonly state
    /// that would otherwise fail the assignments proves nothing was written. (A shell that
    /// never reads commands interactively is the caller's business, and the `-c` and script
    /// cases are covered end to end by `tests/cases/brush/zsh_hooks.yaml`.)
    #[tokio::test]
    async fn init_is_inert_when_hooks_are_off() -> Result<(), ShellError> {
        // Enabled, but the shell isn't interactive in the `$-` sense.
        let mut shell = shell_with_readonly(false, "bash_preexec_imported").await?;
        init(&mut shell, &hooks_enabled())?;
        assert!(shell.env_var(PRECMD_FUNCTIONS).is_none());

        // Interactive, but the option is off.
        let mut shell = shell_with_readonly(true, "bash_preexec_imported").await?;
        init(&mut shell, &InteractiveOptions::default())?;
        assert!(shell.env_var(PRECMD_FUNCTIONS).is_none());

        Ok(())
    }
}
