//! Integration tests for injecting a custom `ExternalCommandSpawner`: that it receives every
//! external command the shell runs, that it can substitute the spawned process, and that the
//! error it returns is mapped to the command's exit status.

#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Result;
use brush_core::extensions::{
    DefaultErrorFormatter, DefaultExternalCommandSpawner, ExternalCommandSpawner,
    ShellExtensionsImpl,
};

/// Returns the string value of the named shell variable, if it's set.
fn var<SE: brush_core::ShellExtensions>(
    shell: &brush_core::Shell<SE>,
    name: &str,
) -> Option<String> {
    shell
        .env_var(name)
        .map(|v| v.value().to_cow_str(shell).into_owned())
}

/// A spawner that records the program and arguments of every command it is asked to spawn,
/// then spawns it unchanged.
#[derive(Clone, Default)]
struct RecordingSpawner {
    seen: Arc<Mutex<Vec<String>>>,
}

impl ExternalCommandSpawner for RecordingSpawner {
    fn spawn(
        &self,
        command: std::process::Command,
        kill_on_drop: bool,
    ) -> std::io::Result<brush_core::sys::process::Child> {
        let program = std::path::Path::new(command.get_program())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let argv = std::iter::once(program)
            .chain(command.get_args().map(|a| a.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(argv);

        DefaultExternalCommandSpawner.spawn(command, kill_on_drop)
    }
}

#[tokio::test]
async fn custom_spawner_sees_every_external_command() -> Result<()> {
    let spawner = RecordingSpawner::default();

    let mut shell = brush_core::Shell::builder_with_extensions::<
        ShellExtensionsImpl<DefaultErrorFormatter, RecordingSpawner>,
    >()
    .external_command_spawner(spawner.clone())
    .build()
    .await?;

    let params = shell.default_exec_params();
    shell
        .run_string(
            "f() { true inner; }; f; true a b | true c; x=$(true subst)",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;

    let seen = spawner
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();

    assert_eq!(seen, vec!["true inner", "true a b", "true c", "true subst"]);

    Ok(())
}

/// A spawner that rewrites or refuses commands based on their first argument: `flip` swaps
/// the program for `false`, `missing` reports the program as not found, and anything else is
/// spawned unchanged.
#[derive(Clone, Default)]
struct RewritingSpawner;

impl ExternalCommandSpawner for RewritingSpawner {
    fn spawn(
        &self,
        command: std::process::Command,
        kill_on_drop: bool,
    ) -> std::io::Result<brush_core::sys::process::Child> {
        match command.get_args().next().and_then(|a| a.to_str()) {
            Some("flip") => DefaultExternalCommandSpawner
                .spawn(std::process::Command::new("false"), kill_on_drop),
            Some("missing") => Err(std::io::ErrorKind::NotFound.into()),
            _ => DefaultExternalCommandSpawner.spawn(command, kill_on_drop),
        }
    }
}

#[tokio::test]
async fn custom_spawner_result_becomes_exit_status() -> Result<()> {
    let mut shell = brush_core::Shell::builder_with_extensions::<
        ShellExtensionsImpl<DefaultErrorFormatter, RewritingSpawner>,
    >()
    .build()
    .await?;

    let params = shell.default_exec_params();
    shell
        .run_string(
            "true flip; flipped=$?; true missing; missing=$?; true; ok=$?",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;

    assert_eq!(var(&shell, "flipped").as_deref(), Some("1"));
    assert_eq!(var(&shell, "missing").as_deref(), Some("127"));
    assert_eq!(var(&shell, "ok").as_deref(), Some("0"));

    Ok(())
}
