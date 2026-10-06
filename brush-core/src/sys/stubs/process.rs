//! Process management utilities

pub(crate) type ProcessId = i32;

/// Provides access to a child process.
pub struct Child {
    inner: std::process::Child,
}

pub(crate) use std::process::ExitStatus;
pub(crate) use std::process::Output;

impl Child {
    /// Returns the process ID of the child process, if available.
    pub fn id(&self) -> Option<u32> {
        None
    }

    /// Asynchronously waits for the child process to exit.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.inner.wait()
    }

    /// Asynchronously waits for the child process to exit and collects its
    /// output.
    pub async fn wait_with_output(self) -> std::io::Result<Output> {
        self.inner.wait_with_output()
    }
}

/// Creates a command to run the executable at `program` as a child of the shell, starting in
/// `working_dir` (the shell's working directory) rather than the host process's. If
/// `working_dir` is empty (the shell doesn't know where it is), the child starts in the host
/// process's.
///
/// `program` is a path the shell has already found -- absolute, or relative to `working_dir`
/// -- never a name to search for.
///
/// # Arguments
///
/// * `program` - The path of the executable to run.
/// * `working_dir` - The directory the child starts in.
pub(crate) fn create_command(
    program: impl AsRef<std::path::Path>,
    working_dir: &crate::ResolvedPath,
) -> std::process::Command {
    let mut command = std::process::Command::new(working_dir.join(program.as_ref()).as_path());
    if !working_dir.is_empty() {
        command.current_dir(working_dir);
    }
    command
}

pub(crate) fn spawn(
    mut command: std::process::Command,
    kill_on_drop: bool,
) -> std::io::Result<Child> {
    // No kill-on-drop on this stub platform; accepted and ignored.
    let _ = kill_on_drop;
    let child = command.spawn()?;
    Ok(Child { inner: child })
}
