//! Integration tests for launching processes through a host's `ProcessLauncher`.

#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};
use brush_core::{
    ExecutionResult, ShellFd,
    extensions::{DefaultErrorFormatter, ProcessLauncher, ShellExtensionsImpl},
    openfiles::OpenFiles,
    processes::{ChildProcess, LaunchRequest, ProcessGroup},
};

/// How a process was to be launched.
#[derive(Debug, PartialEq, Eq)]
enum How {
    Spawn,
    Exec,
}

/// What a launcher was asked to launch.
#[derive(Debug)]
struct Launch {
    how: How,
    program: PathBuf,
    argv0: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    working_dir: PathBuf,
    /// The descriptors the process was to have open, sorted.
    fds: Vec<ShellFd>,
    process_group: ProcessGroup,
}

impl Launch {
    fn new(how: How, request: LaunchRequest) -> Self {
        Self {
            how,
            program: request.program,
            argv0: request.argv0,
            args: request.args,
            env: request.env,
            working_dir: request.working_dir,
            fds: sorted_fds(&request.open_files),
            process_group: request.process_group,
        }
    }

    /// Returns the values of the environment variable `name`, in order.
    fn env_values(&self, name: &str) -> Vec<&str> {
        self.env
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }
}

fn sorted_fds(open_files: &OpenFiles) -> Vec<ShellFd> {
    let mut fds: Vec<_> = open_files.iter_fds().map(|(fd, _)| fd).collect();
    fds.sort_unstable();
    fds
}

/// How the processes a launcher spawns fail, if they do.
#[derive(Clone, Copy, Default)]
enum Failure {
    /// They don't: each exits with 42.
    #[default]
    None,
    /// Starting them fails with an error of this kind.
    Spawn(std::io::ErrorKind),
    /// They start, but waiting for them fails.
    Wait,
}

/// Records what it's asked to launch, and reports each process it spawns as
/// exiting with 42 (unless told to fail).
#[derive(Clone, Default)]
struct RecordingLauncher {
    launches: Arc<Mutex<Vec<Launch>>>,
    /// Whether it can replace the shell's process. If it can, it still fails to,
    /// as if the program couldn't be run, so that the shell carries on.
    can_exec: bool,
    failure: Failure,
}

impl RecordingLauncher {
    fn record(&self, how: How, request: LaunchRequest) -> std::io::Result<()> {
        self.launches
            .lock()
            .map_err(|_| std::io::Error::other("lock poisoned"))?
            .push(Launch::new(how, request));
        Ok(())
    }

    /// Takes what it's been asked to launch so far.
    fn take(&self) -> Result<Vec<Launch>> {
        let mut launches = self
            .launches
            .lock()
            .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
        Ok(std::mem::take(&mut *launches))
    }
}

impl ProcessLauncher for RecordingLauncher {
    fn spawn(&self, request: LaunchRequest) -> Result<ChildProcess, std::io::Error> {
        self.record(How::Spawn, request)?;

        let result = match self.failure {
            Failure::None => Ok(ExecutionResult::new(42)),
            Failure::Spawn(kind) => return Err(kind.into()),
            Failure::Wait => Err(std::io::Error::other("lost track of the process")),
        };
        Ok(ChildProcess::from_future(std::future::ready(result)))
    }

    fn exec(&self, request: LaunchRequest) -> std::io::Error {
        if let Err(err) = self.record(How::Exec, request) {
            return err;
        }

        if self.can_exec {
            std::io::ErrorKind::PermissionDenied.into()
        } else {
            std::io::ErrorKind::Unsupported.into()
        }
    }
}

type Host = ShellExtensionsImpl<DefaultErrorFormatter, RecordingLauncher>;

/// Runs `script` in a shell that launches processes with `launcher`, returning
/// the shell.
async fn run(launcher: &RecordingLauncher, script: &str) -> Result<brush_core::Shell<Host>> {
    let mut shell = brush_core::Shell::builder_with_extensions::<Host>()
        .default_builtins(BuiltinSet::BashMode)
        .process_launcher(launcher.clone())
        .build()
        .await?;

    let params = shell.default_exec_params();
    // The launches are under test; the recorded processes' statuses aren't.
    let _ = shell
        .run_string(script, &brush_core::SourceInfo::default(), &params)
        .await?;

    Ok(shell)
}

/// Runs `script` as [`run`] does, returning the one process it launched.
async fn run_one(launcher: &RecordingLauncher, script: &str) -> Result<Launch> {
    run(launcher, script).await?;

    let mut launches = launcher.take()?.into_iter();
    let launch = launches.next().context("nothing was launched")?;
    assert!(
        launches.next().is_none(),
        "more than one process was launched"
    );
    Ok(launch)
}

#[tokio::test]
async fn external_commands_are_launched_by_the_hosts_launcher() -> Result<()> {
    let launcher = RecordingLauncher::default();
    let shell = run(&launcher, "GREETING=hi ./prog a 'b c'").await?;

    assert_eq!(shell.last_exit_status(), 42);
    let mut launches = launcher.take()?.into_iter();
    let launch = launches.next().context("nothing was launched")?;
    assert!(launches.next().is_none());
    assert_eq!(launch.how, How::Spawn);
    assert_eq!(launch.program, Path::new("./prog"));
    assert_eq!(launch.argv0, "./prog");
    assert_eq!(launch.args, ["a", "b c"]);
    assert_eq!(launch.env_values("GREETING"), ["hi"]);
    assert_eq!(launch.fds, [0, 1, 2]);

    Ok(())
}

#[tokio::test]
async fn a_closed_stream_is_launched_closed() -> Result<()> {
    let launch = run_one(&RecordingLauncher::default(), "./prog <&-").await?;

    assert_eq!(launch.fds, [1, 2]);

    Ok(())
}

#[tokio::test]
async fn a_standard_stream_the_shell_lacks_is_launched_closed() -> Result<()> {
    // As in a shell an embedder made without any open files: its processes get none
    // of the shell process's own, as they'd get none of a stream the shell closed.
    let launcher = RecordingLauncher::default();
    let mut shell = brush_core::Shell::builder_with_extensions::<Host>()
        .default_builtins(BuiltinSet::BashMode)
        .process_launcher(launcher.clone())
        .build()
        .await?;
    *shell.open_files_mut() = OpenFiles::default();

    let params = shell.default_exec_params();
    // The launches are under test; the recorded processes' statuses aren't.
    let _ = shell
        .run_string(
            "./prog\n./prog >&-",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;

    let launches = launcher.take()?;
    let fds: Vec<_> = launches.iter().map(|l| l.fds.clone()).collect();
    assert_eq!(fds, [Vec::<ShellFd>::new(), vec![]]);

    Ok(())
}

#[tokio::test]
async fn a_stream_the_shell_closed_is_launched_closed() -> Result<()> {
    let launch = run_one(&RecordingLauncher::default(), "exec 1>&-\n./prog").await?;

    assert_eq!(launch.fds, [0, 2]);

    Ok(())
}

#[tokio::test]
async fn other_open_files_are_launched_open() -> Result<()> {
    let launch = run_one(&RecordingLauncher::default(), "./prog 3<&0 4>&1").await?;

    assert_eq!(launch.fds, [0, 1, 2, 3, 4]);

    Ok(())
}

#[tokio::test]
async fn an_exported_underscore_is_launched_once() -> Result<()> {
    let launcher = RecordingLauncher::default();
    let mut shell = shell_with_inherited_underscore(&launcher).await?;

    let params = shell.default_exec_params();
    // The launches are under test; the recorded processes' statuses aren't.
    let _ = shell
        .run_string("./prog", &brush_core::SourceInfo::default(), &params)
        .await?;

    let launches = launcher.take()?;
    let launch = launches.first().context("nothing was launched")?;
    assert_eq!(launch.env_values("_"), ["./prog"]);

    Ok(())
}

/// Builds a shell, as [`run`] does, whose `_` is exported, as it is when a shell
/// inherits it from its parent's environment.
async fn shell_with_inherited_underscore(
    launcher: &RecordingLauncher,
) -> Result<brush_core::Shell<Host>> {
    let mut underscore = brush_core::ShellVariable::new("/from/parent");
    underscore.export();
    Ok(brush_core::Shell::builder_with_extensions::<Host>()
        .default_builtins(BuiltinSet::BashMode)
        .var("_", underscore)
        .process_launcher(launcher.clone())
        .build()
        .await?)
}

#[tokio::test]
async fn exec_leaves_underscore_as_the_shell_exports_it() -> Result<()> {
    let launcher = RecordingLauncher {
        can_exec: true,
        ..Default::default()
    };
    let mut shell = shell_with_inherited_underscore(&launcher).await?;
    let params = shell.default_exec_params();
    // The launches are under test; the recorded processes' statuses aren't.
    let _ = shell
        .run_string("exec ./prog", &brush_core::SourceInfo::default(), &params)
        .await?;

    let launches = launcher.take()?;
    let launch = launches.first().context("nothing was launched")?;
    assert_eq!(launch.env_values("_"), ["/from/parent"]);

    Ok(())
}

#[tokio::test]
async fn exec_hands_the_launcher_its_request() -> Result<()> {
    let launcher = RecordingLauncher {
        can_exec: true,
        ..Default::default()
    };

    let launch = run_one(&launcher, "exec -a NAME ./prog x").await?;
    assert_eq!(launch.how, How::Exec);
    assert_eq!(launch.program, Path::new("./prog"));
    assert_eq!(launch.argv0, "NAME");
    assert_eq!(launch.args, ["x"]);

    let launch = run_one(&launcher, "exec -l ./prog").await?;
    assert_eq!(launch.argv0, "-./prog");

    let launch = run_one(&launcher, "export GREETING=hi\nexec -c ./prog").await?;
    assert_eq!(launch.env, []);

    Ok(())
}

#[tokio::test]
async fn exec_launches_the_program_running_it_would() -> Result<()> {
    let launcher = RecordingLauncher {
        can_exec: true,
        ..Default::default()
    };

    run(&launcher, "./prog\nexec ./prog").await?;

    let launches = launcher.take()?;
    let hows: Vec<_> = launches.iter().map(|l| &l.how).collect();
    assert_eq!(hows, [&How::Spawn, &How::Exec]);
    assert_eq!(launches[1].program, launches[0].program);

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn exec_runs_only_the_program_it_resolved() -> Result<()> {
    // `printf` is a builtin too, and the builtin would set `out`; but `exec` runs
    // the program found in `PATH`.
    let launcher = RecordingLauncher {
        can_exec: true,
        ..Default::default()
    };
    let shell = run(&launcher, "exec printf -v out hi").await?;

    assert_eq!(shell.env_str("out"), None);
    let launches = launcher.take()?;
    let [launch] = launches.as_slice() else {
        anyhow::bail!("expected one launch, got {}", launches.len());
    };
    assert_eq!(launch.how, How::Exec);
    assert!(launch.program.is_absolute());
    assert_eq!(launch.program.file_name(), Some("printf".as_ref()));
    assert_eq!(launch.argv0, "printf");

    Ok(())
}

#[tokio::test]
async fn every_way_of_running_a_command_uses_the_hosts_launcher() -> Result<()> {
    let launcher = RecordingLauncher::default();
    run(&launcher, "x=$(./a)\n./b | ./c\n(./d)\n./e &\nwait").await?;

    let mut programs: Vec<_> = launcher.take()?.into_iter().map(|l| l.program).collect();
    programs.sort();
    assert_eq!(
        programs,
        ["./a", "./b", "./c", "./d", "./e"].map(PathBuf::from)
    );

    Ok(())
}

#[tokio::test]
async fn a_process_starts_in_the_shells_working_dir() -> Result<()> {
    let launcher = RecordingLauncher::default();
    let script = std::format!("cd '{}'\n./prog", std::env::temp_dir().display());
    let shell = run(&launcher, &script).await?;

    // Compare with where the shell ended up, not the path given to `cd`, which a
    // platform may spell differently (e.g., through a symlink on macOS).
    let launches = launcher.take()?;
    let launch = launches.first().context("nothing was launched")?;
    assert_ne!(launch.working_dir, std::env::current_dir()?);
    assert_eq!(launch.working_dir, shell.working_dir());

    Ok(())
}

#[tokio::test]
async fn without_job_control_processes_stay_in_the_shells_group() -> Result<()> {
    let launcher = RecordingLauncher::default();
    run(&launcher, "./a\n./b | ./c").await?;

    for launch in launcher.take()? {
        assert_eq!(launch.process_group, ProcessGroup::Inherit, "{launch:?}");
    }

    Ok(())
}

#[tokio::test]
async fn with_job_control_a_command_gets_a_group_of_its_own() -> Result<()> {
    let launcher = RecordingLauncher::default();
    let mut shell = brush_core::Shell::builder_with_extensions::<Host>()
        .process_launcher(launcher.clone())
        .build()
        .await?;
    shell.options_mut().enable_job_control = true;

    let params = shell.default_exec_params();
    // The launches are under test; the recorded processes' statuses aren't.
    let _ = shell
        .run_string("./prog", &brush_core::SourceInfo::default(), &params)
        .await?;

    let launches = launcher.take()?;
    let launch = launches.first().context("nothing was launched")?;
    assert!(
        matches!(launch.process_group, ProcessGroup::New { .. }),
        "{:?}",
        launch.process_group
    );

    Ok(())
}

#[tokio::test]
async fn a_program_the_launcher_cant_find_is_a_command_not_found() -> Result<()> {
    let launcher = RecordingLauncher {
        failure: Failure::Spawn(std::io::ErrorKind::NotFound),
        ..Default::default()
    };
    let shell = run(&launcher, "./prog").await?;

    assert_eq!(shell.last_exit_status(), 127);

    Ok(())
}

#[tokio::test]
async fn a_program_the_launcher_cant_start_cant_be_executed() -> Result<()> {
    let launcher = RecordingLauncher {
        failure: Failure::Spawn(std::io::ErrorKind::PermissionDenied),
        ..Default::default()
    };
    let shell = run(&launcher, "./prog").await?;

    assert_eq!(shell.last_exit_status(), 126);

    Ok(())
}

#[tokio::test]
async fn a_process_that_cant_be_waited_for_fails_the_command() -> Result<()> {
    let launcher = RecordingLauncher {
        failure: Failure::Wait,
        ..Default::default()
    };
    let shell = run(&launcher, "./prog\nstatus=$?").await?;

    assert_eq!(shell.env_str("status").as_deref(), Some("1"));

    Ok(())
}

#[tokio::test]
#[ignore = "TODO(exec): exec fails where the launcher can't replace the shell's process"]
async fn exec_with_a_launcher_that_cant_exec_runs_the_program_then_exits() -> Result<()> {
    let launcher = RecordingLauncher::default();
    let shell = run(&launcher, "exec ./a x\n./b").await?;

    assert_eq!(shell.last_exit_status(), 42);
    let spawned: Vec<_> = launcher
        .take()?
        .into_iter()
        .filter(|l| l.how == How::Spawn)
        .map(|l| (l.program, l.argv0, l.args))
        .collect();
    assert_eq!(
        spawned,
        [(PathBuf::from("./a"), "./a".to_owned(), vec!["x".to_owned()])]
    );

    Ok(())
}
