//! Process management utilities

use std::process::Stdio;

use futures::FutureExt as _;

use crate::{
    ExecutionResult, ShellFd,
    openfiles::OpenFiles,
    processes::{ChildProcess, LaunchRequest, ProcessGroup},
    sys::commands::{self, CommandExt, CommandFdInjectionExt, CommandFgControlExt},
};

pub(crate) type ProcessId = i32;

/// Starts a native child process.
///
/// # Arguments
///
/// * `command` - The process to start.
pub(crate) fn spawn(command: LaunchRequest) -> std::io::Result<ChildProcess> {
    let process_group = command.process_group;
    // See `CreateOptions::kill_external_commands_on_drop` (false for ordinary shells).
    let kill_on_drop = command.kill_on_drop;

    let mut command = tokio::process::Command::from(compose_std_command(command)?);
    command.kill_on_drop(kill_on_drop);
    let child = command.spawn()?;

    #[expect(clippy::cast_possible_wrap)]
    let pid = child.id().map(|id| id as ProcessId);
    if pid.is_none() {
        tracing::warn!("could not retrieve pid for child process");
    }

    let pgid = match process_group {
        ProcessGroup::Inherit => None,
        ProcessGroup::Join(pgid) => Some(pgid),
        ProcessGroup::New { .. } | ProcessGroup::NewSession => pid,
    };

    let exited = child
        .wait_with_output()
        .map(|output| output.map(ExecutionResult::from));
    Ok(ChildProcess::from_future(exited).native_child(pid, pgid))
}

/// Replaces the current process with a new one. Returns only if that fails, with
/// the reason why.
///
/// # Arguments
///
/// * `command` - The process to replace the current one with.
pub(crate) fn exec(command: LaunchRequest) -> std::io::Error {
    match compose_std_command(command) {
        Ok(command) => commands::exec(command),
        Err(err) => err,
    }
}

/// Composes the `std::process::Command` that starts a process.
fn compose_std_command(command: LaunchRequest) -> std::io::Result<std::process::Command> {
    // A relative program is relative to the working directory.
    let program = commands::program_to_start(command.program, &command.working_dir);
    let mut cmd = std::process::Command::new(program);

    // Override argv[0].
    // NOTE: Not supported on all platforms.
    cmd.arg0(command.argv0);
    cmd.args(command.args);
    cmd.current_dir(command.working_dir);
    cmd.env_clear();
    cmd.envs(command.env);

    // A closed standard stream gets the null device.
    // TODO(redirection): close it instead, as bash does (e.g., with a `pre_exec`
    // on Unix), so that the process fails to use it.
    let mut open_files = command.open_files;
    cmd.stdin(take_stdio(&mut open_files, OpenFiles::STDIN_FD)?);
    cmd.stdout(take_stdio(&mut open_files, OpenFiles::STDOUT_FD)?);
    cmd.stderr(take_stdio(&mut open_files, OpenFiles::STDERR_FD)?);

    cmd.inject_fds(open_files.iter_fds().map(|(fd, file)| (fd, file.clone())))
        .map_err(std::io::Error::other)?;

    match command.process_group {
        ProcessGroup::Inherit => {}
        ProcessGroup::Join(pgid) => {
            cmd.process_group(pgid);
        }
        ProcessGroup::New { take_foreground } => {
            cmd.process_group(0);
            if take_foreground {
                cmd.take_foreground();
            }
        }
        // setsid() makes a new process group too.
        ProcessGroup::NewSession => cmd.lead_session(),
    }

    Ok(cmd)
}

/// Takes the file for standard stream `fd` out of `open_files`, for the process
/// to have as that stream: the null device if it's closed.
fn take_stdio(open_files: &mut OpenFiles, fd: ShellFd) -> std::io::Result<Stdio> {
    match open_files.remove_fd(fd) {
        Some(file) => commands::stdio_for(file),
        None => Ok(Stdio::null()),
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn program_started_for(program: &Path, working_dir: &Path) -> std::io::Result<PathBuf> {
        let request = LaunchRequest {
            program: program.to_owned(),
            argv0: String::from("prog"),
            args: vec![],
            env: vec![],
            working_dir: working_dir.to_owned(),
            open_files: OpenFiles::default(),
            process_group: ProcessGroup::Inherit,
            kill_on_drop: false,
        };

        Ok(PathBuf::from(compose_std_command(request)?.get_program()))
    }

    /// A relative program is started from the request's working directory, which
    /// needn't be the shell process's (and may itself be relative). On Unix, where a
    /// relative program is looked for after the change of directory, it keeps the
    /// path it's given, which a script sees as its `$0`; elsewhere, it's put under the
    /// working directory. A bare name and an absolute path are left as they are.
    #[test]
    fn relative_program_is_relative_to_the_working_dir() -> std::io::Result<()> {
        let relative = Path::new("bin").join("prog");
        for working_dir in [std::env::temp_dir(), PathBuf::from("sub")] {
            let expected = if cfg!(unix) {
                relative.clone()
            } else {
                working_dir.join(&relative)
            };
            assert_eq!(program_started_for(&relative, &working_dir)?, expected);

            let absolute = std::env::temp_dir().join("elsewhere").join("prog");
            assert_eq!(program_started_for(&absolute, &working_dir)?, absolute);
            assert_eq!(
                program_started_for(Path::new("prog"), &working_dir)?,
                Path::new("prog")
            );
        }

        Ok(())
    }
}
