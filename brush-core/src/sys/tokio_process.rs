//! Process management utilities

pub(crate) type ProcessId = i32;
pub(crate) use tokio::process::Child;

/// Spawns the given command.
///
/// # Arguments
///
/// * `command` - The command to spawn.
/// * `kill_on_drop` - Whether the spawned process should be killed when the
///   returned [`Child`] handle is dropped. See
///   [`CreateOptions::kill_external_commands_on_drop`](crate::CreateOptions::kill_external_commands_on_drop)
///   for when this is appropriate; it is `false` for ordinary shells, which must
///   outlive their children.
pub(crate) fn spawn(command: std::process::Command, kill_on_drop: bool) -> std::io::Result<Child> {
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(kill_on_drop);
    command.spawn()
}
