//! Process management utilities

pub(crate) type ProcessId = i32;
pub(crate) use tokio::process::Child;

// `kill_on_drop`: see `CreateOptions::kill_external_commands_on_drop` (false for ordinary shells).
pub(crate) fn spawn(command: std::process::Command, kill_on_drop: bool) -> std::io::Result<Child> {
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(kill_on_drop);
    command.spawn()
}
