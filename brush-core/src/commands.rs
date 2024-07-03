#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{ffi::OsStr, fmt::Display, process::Stdio, sync::Arc};

use brush_parser::ast;
#[cfg(unix)]
use command_fds::{CommandFdExt, FdMapping};
use itertools::Itertools;

use crate::{
    builtin, error,
    interp::{self, Execute},
    openfiles::{self, OpenFile, OpenFiles},
    sys, Shell,
};

/// Represents the result of spawning a command.
pub(crate) enum SpawnResult {
    /// The child process was spawned.
    SpawnedChild(sys::process::Child),
    /// The command immediatedly exited with the given numeric exit code.
    ImmediateExit(u8),
    /// The shell should exit after this command, yielding the given numeric exit code.
    ExitShell(u8),
    /// The shell should return from the current function or script, yielding the given numeric
    /// exit code.
    ReturnFromFunctionOrScript(u8),
    /// The shell should break out of the containing loop, identified by the given depth count.
    BreakLoop(u8),
    /// The shell should continue the containing loop, identified by the given depth count.
    ContinueLoop(u8),
}

/// Represents the context for executing a command.
pub struct ExecutionContext<'a> {
    /// The shell in which the command is being executed.
    pub shell: &'a mut Shell,
    /// The name of the command being executed.    
    pub command_name: String,
    /// The open files tracked by the current context.
    pub open_files: openfiles::OpenFiles,
}

impl ExecutionContext<'_> {
    /// Returns the standard input file; usable with `write!` et al.
    pub fn stdin(&self) -> openfiles::OpenFile {
        self.fd(0).unwrap()
    }

    /// Returns the standard output file; usable with `write!` et al.
    pub fn stdout(&self) -> openfiles::OpenFile {
        self.fd(1).unwrap()
    }

    /// Returns the standard error file; usable with `write!` et al.
    pub fn stderr(&self) -> openfiles::OpenFile {
        self.fd(2).unwrap()
    }

    /// Returns the file descriptor with the given number.
    pub fn fd(&self, fd: u32) -> Option<openfiles::OpenFile> {
        self.open_files.files.get(&fd).map(|f| f.try_dup().unwrap())
    }
}

/// An argument to a command.
#[derive(Clone, Debug)]
pub enum CommandArg {
    /// A simple string argument.
    String(String),
    /// An assignment/declaration; typically treated as a string, but will
    /// be specially handled by a limited set of built-in commands.
    Assignment(ast::Assignment),
}

impl Display for CommandArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandArg::String(s) => f.write_str(s),
            CommandArg::Assignment(a) => write!(f, "{a}"),
        }
    }
}

impl From<String> for CommandArg {
    fn from(s: String) -> Self {
        CommandArg::String(s)
    }
}

impl From<&String> for CommandArg {
    fn from(value: &String) -> Self {
        CommandArg::String(value.clone())
    }
}

#[allow(unused_variables)]
pub(crate) fn compose_std_command<S: AsRef<OsStr>>(
    shell: &mut Shell,
    command_name: &str,
    argv0: &str,
    args: &[S],
    mut open_files: OpenFiles,
    empty_env: bool,
) -> Result<std::process::Command, error::Error> {
    let mut cmd = std::process::Command::new(command_name);

    // Override argv[0].
    #[cfg(unix)]
    cmd.arg0(argv0);

    // Pass through args.
    for arg in args {
        cmd.arg(arg);
    }

    // Use the shell's current working dir.
    cmd.current_dir(shell.working_dir.as_path());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    if !empty_env {
        for (name, var) in shell.env.iter() {
            if var.is_exported() {
                let value_as_str = var.value().to_cow_string();
                cmd.env(name, value_as_str.as_ref());
            }
        }
    }

    // Redirect stdin, if applicable.
    match open_files.files.remove(&0) {
        Some(OpenFile::Stdin) | None => (),
        Some(stdin_file) => {
            let as_stdio: Stdio = stdin_file.into();
            cmd.stdin(as_stdio);
        }
    }

    // Redirect stdout, if applicable.
    match open_files.files.remove(&1) {
        Some(OpenFile::Stdout) | None => (),
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.into();
            cmd.stdout(as_stdio);
        }
    }

    // Redirect stderr, if applicable.
    match open_files.files.remove(&2) {
        Some(OpenFile::Stderr) | None => {}
        Some(stderr_file) => {
            let as_stdio: Stdio = stderr_file.into();
            cmd.stderr(as_stdio);
        }
    }

    // Inject any other fds.
    #[cfg(unix)]
    {
        let fd_mappings = open_files
            .files
            .into_iter()
            .map(|(child_fd, open_file)| FdMapping {
                child_fd: i32::try_from(child_fd).unwrap(),
                parent_fd: open_file.into_owned_fd().unwrap(),
            })
            .collect();
        cmd.fd_mappings(fd_mappings)
            .map_err(|_e| error::Error::ChildCreationFailure)?;
    }
    #[cfg(not(unix))]
    {
        if !open_files.files.is_empty() {
            return error::unimp("fd redirections on non-Unix platform");
        }
    }

    Ok(cmd)
}

pub(crate) async fn execute(
    cmd_context: ExecutionContext<'_>,
    args: Vec<CommandArg>,
    use_functions: bool,
) -> Result<SpawnResult, error::Error> {
    if !cmd_context.command_name.contains(std::path::MAIN_SEPARATOR) {
        let builtin = cmd_context
            .shell
            .builtins
            .get(&cmd_context.command_name)
            .cloned();

        // Ignore the builtin if it's marked as disabled.
        if builtin
            .as_ref()
            .is_some_and(|r| !r.disabled && r.special_builtin)
        {
            return execute_builtin_command(&builtin.unwrap(), cmd_context, args).await;
        }

        if use_functions {
            if let Some(func_reg) = cmd_context
                .shell
                .funcs
                .get(cmd_context.command_name.as_str())
            {
                // Strip the function name off args.
                return invoke_shell_function(func_reg.definition.clone(), cmd_context, &args[1..])
                    .await;
            }
        }

        if let Some(builtin) = builtin {
            if !builtin.disabled {
                return execute_builtin_command(&builtin, cmd_context, args).await;
            }
        }
    }

    // Strip the command name off args.
    execute_external_command(cmd_context, &args[1..])
}

#[allow(clippy::too_many_lines)]
pub(crate) fn execute_external_command(
    context: ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<SpawnResult, error::Error> {
    // Filter out the args; we only want strings.
    let mut cmd_args = vec![];
    for arg in args {
        if let CommandArg::String(s) = arg {
            cmd_args.push(s);
        }
    }

    // Compose the std::process::Command that encapsulates what we want to launch.
    let cmd = compose_std_command(
        context.shell,
        context.command_name.as_str(),
        context.command_name.as_str(),
        cmd_args.as_slice(),
        context.open_files,
        false, /* empty environment? */
    )?;

    // When tracing is enabled, report.
    tracing::debug!(
        target: "commands",
        "Spawning: {} {}",
        cmd.get_program().to_string_lossy().to_string(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .join(" ")
    );

    match sys::process::spawn(cmd) {
        Ok(child) => Ok(SpawnResult::SpawnedChild(child)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if context.shell.options.sh_mode {
                tracing::error!(
                    "{}: {}: {}: not found",
                    context.shell.shell_name.as_ref().unwrap_or(&String::new()),
                    context.shell.get_current_input_line_number(),
                    context.command_name
                );
            } else {
                tracing::error!("{}: not found", context.command_name);
            }
            Ok(SpawnResult::ImmediateExit(127))
        }
        Err(e) => {
            tracing::error!("error: {}", e);
            Ok(SpawnResult::ImmediateExit(126))
        }
    }
}

async fn execute_builtin_command(
    builtin: &builtin::Registration,
    context: ExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> Result<SpawnResult, error::Error> {
    let exit_code = match (builtin.execute_func)(context, args).await {
        Ok(builtin_result) => match builtin_result.exit_code {
            builtin::ExitCode::Success => 0,
            builtin::ExitCode::InvalidUsage => 2,
            builtin::ExitCode::Unimplemented => 99,
            builtin::ExitCode::Custom(code) => code,
            builtin::ExitCode::ExitShell(code) => return Ok(SpawnResult::ExitShell(code)),
            builtin::ExitCode::ReturnFromFunctionOrScript(code) => {
                return Ok(SpawnResult::ReturnFromFunctionOrScript(code))
            }
            builtin::ExitCode::BreakLoop(count) => return Ok(SpawnResult::BreakLoop(count)),
            builtin::ExitCode::ContinueLoop(count) => return Ok(SpawnResult::ContinueLoop(count)),
        },
        Err(e) => {
            tracing::error!("error: {}", e);
            1
        }
    };

    Ok(SpawnResult::ImmediateExit(exit_code))
}

pub(crate) async fn invoke_shell_function(
    function_definition: Arc<ast::FunctionDefinition>,
    mut context: ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<SpawnResult, error::Error> {
    let ast::FunctionBody(body, redirects) = &function_definition.body;

    // Apply any redirects specified at function definition-time.
    if let Some(redirects) = redirects {
        for redirect in &redirects.0 {
            interp::setup_redirect(&mut context.open_files, context.shell, redirect).await?;
        }
    }

    // Temporarily replace positional parameters.
    let prior_positional_params = std::mem::take(&mut context.shell.positional_parameters);
    context.shell.positional_parameters = args.iter().map(|a| a.to_string()).collect();

    // Pass through open files.
    let params = interp::ExecutionParameters {
        open_files: context.open_files.clone(),
    };

    // Note that we're going deeper. Once we do this, we need to make sure we don't bail early
    // before "exiting" the function.
    context
        .shell
        .enter_function(context.command_name.as_str(), &function_definition)?;

    // Invoke the function.
    let result = body.execute(context.shell, &params).await;

    // Clean up parameters so any owned files are closed.
    drop(params);

    // We've come back out, reflect it.
    context.shell.leave_function()?;

    // Restore positional parameters.
    context.shell.positional_parameters = prior_positional_params;

    Ok(SpawnResult::ImmediateExit(result?.exit_code))
}
