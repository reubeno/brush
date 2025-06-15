use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{borrow::Cow, ffi::OsStr, fmt::Display, process::Stdio, sync::Arc};

use brush_parser::ast;
#[cfg(unix)]
use command_fds::{CommandFdExt, FdMapping};
use itertools::Itertools;

use crate::{
    ExecutionParameters, ExecutionResult, Shell, builtins, error, escape,
    interp::{self, Execute, ProcessGroupPolicy},
    openfiles::{self, OpenFile, OpenFiles},
    processes, sys, trace_categories,
};

/// Represents the result of spawning a command.
pub(crate) enum CommandSpawnResult {
    /// The child process was spawned.
    SpawnedProcess(processes::ChildProcess),
    /// The command immediately exited with the given numeric exit code.
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

impl CommandSpawnResult {
    // TODO: jobs: remove `no_wait`; it doesn't make any sense
    #[allow(clippy::too_many_lines)]
    pub async fn wait(self, no_wait: bool) -> Result<CommandWaitResult, error::Error> {
        #[allow(clippy::ignored_unit_patterns)]
        match self {
            Self::SpawnedProcess(mut child) => {
                let process_wait_result = if !no_wait {
                    // Wait for the process to exit or for a relevant signal, whichever happens
                    // first.
                    child.wait().await?
                } else {
                    processes::ProcessWaitResult::Stopped
                };

                let command_wait_result = match process_wait_result {
                    processes::ProcessWaitResult::Completed(output) => {
                        CommandWaitResult::CommandCompleted(ExecutionResult::from(output))
                    }
                    processes::ProcessWaitResult::Stopped => CommandWaitResult::CommandStopped(
                        ExecutionResult::from(processes::ProcessWaitResult::Stopped),
                        child,
                    ),
                };

                Ok(command_wait_result)
            }
            Self::ImmediateExit(exit_code) => Ok(CommandWaitResult::CommandCompleted(
                ExecutionResult::new(exit_code),
            )),
            Self::ExitShell(exit_code) => {
                Ok(CommandWaitResult::CommandCompleted(ExecutionResult {
                    exit_code,
                    exit_shell: true,
                    ..ExecutionResult::default()
                }))
            }
            Self::ReturnFromFunctionOrScript(exit_code) => {
                Ok(CommandWaitResult::CommandCompleted(ExecutionResult {
                    exit_code,
                    return_from_function_or_script: true,
                    ..ExecutionResult::default()
                }))
            }
            Self::BreakLoop(count) => Ok(CommandWaitResult::CommandCompleted(ExecutionResult {
                exit_code: 0,
                break_loop: Some(count),
                ..ExecutionResult::default()
            })),
            Self::ContinueLoop(count) => Ok(CommandWaitResult::CommandCompleted(ExecutionResult {
                exit_code: 0,
                continue_loop: Some(count),
                ..ExecutionResult::default()
            })),
        }
    }
}

/// Encapsulates the result of waiting for a command to complete.
pub(crate) enum CommandWaitResult {
    /// The command completed.
    CommandCompleted(ExecutionResult),
    /// The command was stopped before it completed.
    CommandStopped(ExecutionResult, processes::ChildProcess),
}

/// Represents the context for executing a command.
pub struct ExecutionContext<'a> {
    /// The shell in which the command is being executed.
    pub shell: &'a mut Shell,
    /// The name of the command being executed.    
    pub command_name: String,
    /// The parameters for the execution.
    pub params: ExecutionParameters,
}

impl ExecutionContext<'_> {
    /// Returns the standard input file; usable with `write!` et al.
    pub fn stdin(&self) -> impl std::io::Read + 'static {
        self.params.stdin()
    }

    /// Returns the standard output file; usable with `write!` et al.
    pub fn stdout(&self) -> impl std::io::Write + 'static {
        self.params.stdout()
    }

    /// Returns the standard error file; usable with `write!` et al.
    pub fn stderr(&self) -> impl std::io::Write + 'static {
        self.params.stderr()
    }

    pub(crate) const fn should_cmd_lead_own_process_group(&self) -> bool {
        self.shell.options.interactive
            && matches!(
                self.params.process_group_policy,
                ProcessGroupPolicy::NewProcessGroup
            )
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
            Self::String(s) => f.write_str(s),
            Self::Assignment(a) => write!(f, "{a}"),
        }
    }
}

impl From<String> for CommandArg {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&String> for CommandArg {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl CommandArg {
    pub(crate) fn quote_for_tracing(&self) -> Cow<'_, str> {
        match self {
            Self::String(s) => escape::quote_if_needed(s, escape::QuoteMode::SingleQuote),
            Self::Assignment(a) => {
                let mut s = a.name.to_string();
                let op = if a.append { "+=" } else { "=" };
                s.push_str(op);
                s.push_str(&escape::quote_if_needed(
                    a.value.to_string().as_str(),
                    escape::QuoteMode::SingleQuote,
                ));
                s.into()
            }
        }
    }
}

#[allow(unused_variables)]
pub(crate) fn compose_std_command<S: AsRef<OsStr>>(
    shell: &Shell,
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
    cmd.args(args);

    // Use the shell's current working dir.
    cmd.current_dir(shell.working_dir.as_path());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    if !empty_env {
        for (k, v) in shell.env.iter_exported() {
            cmd.env(k.as_str(), v.value().to_cow_str(shell).as_ref());
        }
    }

    // Add in exported functions.
    if !empty_env {
        for (func_name, registration) in shell.funcs.iter() {
            if registration.is_exported() {
                let var_name = std::format!("BASH_FUNC_{func_name}%%");
                let value = std::format!("() {}", registration.definition.body);
                cmd.env(var_name, value);
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
    process_group_id: &mut Option<i32>,
    args: Vec<CommandArg>,
    use_functions: bool,
    path_dirs: Option<Vec<String>>,
) -> Result<CommandSpawnResult, error::Error> {
    // First see if it's the name of a builtin.
    let builtin = cmd_context
        .shell
        .builtins
        .get(&cmd_context.command_name)
        .cloned();

    // If we found a special builtin (that's not disabled), then invoke it.
    if builtin
        .as_ref()
        .is_some_and(|r| !r.disabled && r.special_builtin)
    {
        return execute_builtin_command(&builtin.unwrap(), cmd_context, args).await;
    }

    // Assuming we weren't requested not to do so, check if it's the name of
    // a shell function.
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

    // If we found a (non-special) builtin and it's not disabled, then invoke it.
    if let Some(builtin) = builtin {
        if !builtin.disabled {
            return execute_builtin_command(&builtin, cmd_context, args).await;
        }
    }

    // We still haven't found a command to invoke. We'll need to look for an external command.
    if !cmd_context.command_name.contains(std::path::MAIN_SEPARATOR) {
        // All else failed; if we were given path directories to search, try to look through them
        // for a matching executable. Otherwise, use our default search logic.
        let path = if let Some(path_dirs) = path_dirs {
            cmd_context
                .shell
                .find_executables_in(path_dirs.iter(), &cmd_context.command_name)
                .first()
                .cloned()
        } else {
            cmd_context
                .shell
                .find_first_executable_in_path_using_cache(&cmd_context.command_name)
        };

        if let Some(path) = path {
            let resolved_path = path.to_string_lossy();
            execute_external_command(
                cmd_context,
                resolved_path.as_ref(),
                process_group_id,
                &args[1..],
            )
        } else {
            writeln!(
                cmd_context.stderr(),
                "{}: command not found",
                cmd_context.command_name
            )?;
            Ok(CommandSpawnResult::ImmediateExit(127))
        }
    } else {
        let resolved_path = cmd_context.command_name.clone();

        // Strip the command name off args.
        execute_external_command(
            cmd_context,
            resolved_path.as_str(),
            process_group_id,
            &args[1..],
        )
    }
}

#[allow(clippy::too_many_lines)]
#[allow(unused_variables)]
pub(crate) fn execute_external_command(
    context: ExecutionContext<'_>,
    executable_path: &str,
    process_group_id: &mut Option<i32>,
    args: &[CommandArg],
) -> Result<CommandSpawnResult, error::Error> {
    // Filter out the args; we only want strings.
    let mut cmd_args = vec![];
    for arg in args {
        if let CommandArg::String(s) = arg {
            cmd_args.push(s);
        }
    }

    // Before we lose ownership of the open files, figure out if stdin will be a terminal.
    #[allow(unused_variables)]
    let child_stdin_is_terminal = context
        .params
        .open_files
        .stdin()
        .is_some_and(|f| f.is_term());

    // Figure out if we should be setting up a new process group.
    let new_pg = context.should_cmd_lead_own_process_group();

    // Save copy of stderr for errors.
    let mut stderr = context.stderr();

    // Compose the std::process::Command that encapsulates what we want to launch.
    #[allow(unused_mut)]
    let mut cmd = compose_std_command(
        context.shell,
        executable_path,
        context.command_name.as_str(),
        cmd_args.as_slice(),
        context.params.open_files,
        false, /* empty environment? */
    )?;

    // Set up process group state.
    if new_pg {
        // We need to set up a new process group.
        #[cfg(unix)]
        cmd.process_group(0);
    } else if let Some(pgid) = process_group_id {
        // We need to join an established process group.
        #[cfg(unix)]
        cmd.process_group(*pgid);
    }

    // Register some code to run in the forked child process before it execs
    // the target command.
    #[cfg(unix)]
    if new_pg && child_stdin_is_terminal {
        unsafe {
            cmd.pre_exec(setup_process_before_exec);
        }
    }

    // When tracing is enabled, report.
    tracing::debug!(
        target: trace_categories::COMMANDS,
        "Spawning: cmd='{} {}'",
        cmd.get_program().to_string_lossy().to_string(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .join(" ")
    );

    match sys::process::spawn(cmd) {
        Ok(child) => {
            // Retrieve the pid.
            #[allow(clippy::cast_possible_wrap)]
            let pid = child.id().map(|id| id as i32);
            if let Some(pid) = &pid {
                if new_pg {
                    *process_group_id = Some(*pid);
                }
            } else {
                tracing::warn!("could not retrieve pid for child process");
            }

            Ok(CommandSpawnResult::SpawnedProcess(
                processes::ChildProcess::new(pid, child),
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if context.shell.options.interactive {
                sys::terminal::move_self_to_foreground()?;
            }

            if !context.shell.working_dir.exists() {
                // We may have failed because the working directory doesn't exist.
                writeln!(
                    stderr,
                    "{}: working directory does not exist: {}",
                    context.shell.shell_name.as_ref().unwrap_or(&String::new()),
                    context.shell.working_dir.display()
                )?;
            } else if context.shell.options.sh_mode {
                writeln!(
                    stderr,
                    "{}: {}: {}: not found",
                    context.shell.shell_name.as_ref().unwrap_or(&String::new()),
                    context.shell.get_current_input_line_number(),
                    context.command_name
                )?;
            } else {
                writeln!(stderr, "{}: not found", context.command_name)?;
            }
            Ok(CommandSpawnResult::ImmediateExit(127))
        }
        Err(e) => {
            if context.shell.options.interactive {
                sys::terminal::move_self_to_foreground()?;
            }

            tracing::error!("{e}");
            Ok(CommandSpawnResult::ImmediateExit(126))
        }
    }
}

#[cfg(unix)]
fn setup_process_before_exec() -> Result<(), std::io::Error> {
    sys::terminal::move_self_to_foreground().map_err(std::io::Error::other)?;
    Ok(())
}

async fn execute_builtin_command(
    builtin: &builtins::Registration,
    context: ExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> Result<CommandSpawnResult, error::Error> {
    let exit_code = match (builtin.execute_func)(context, args).await {
        Ok(builtin_result) => match builtin_result.exit_code {
            builtins::ExitCode::Success => 0,
            builtins::ExitCode::InvalidUsage => 2,
            builtins::ExitCode::Unimplemented => 99,
            builtins::ExitCode::Custom(code) => code,
            builtins::ExitCode::ExitShell(code) => return Ok(CommandSpawnResult::ExitShell(code)),
            builtins::ExitCode::ReturnFromFunctionOrScript(code) => {
                return Ok(CommandSpawnResult::ReturnFromFunctionOrScript(code));
            }
            builtins::ExitCode::BreakLoop(count) => {
                return Ok(CommandSpawnResult::BreakLoop(count));
            }
            builtins::ExitCode::ContinueLoop(count) => {
                return Ok(CommandSpawnResult::ContinueLoop(count));
            }
        },
        Err(e @ error::Error::Unimplemented(..)) => {
            tracing::warn!(target: trace_categories::UNIMPLEMENTED, "{e}");
            1
        }
        Err(e @ error::Error::UnimplementedAndTracked(..)) => {
            tracing::warn!(target: trace_categories::UNIMPLEMENTED, "{e}");
            1
        }
        Err(e) => {
            tracing::error!("{e}");
            1
        }
    };

    Ok(CommandSpawnResult::ImmediateExit(exit_code))
}

pub(crate) async fn invoke_shell_function(
    function_definition: Arc<ast::FunctionDefinition>,
    mut context: ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<CommandSpawnResult, error::Error> {
    let ast::FunctionBody(body, redirects) = &function_definition.body;

    // Apply any redirects specified at function definition-time.
    if let Some(redirects) = redirects {
        for redirect in &redirects.0 {
            interp::setup_redirect(context.shell, &mut context.params, redirect).await?;
        }
    }

    // Temporarily replace positional parameters.
    let prior_positional_params = std::mem::take(&mut context.shell.positional_parameters);
    context.shell.positional_parameters = args.iter().map(|a| a.to_string()).collect();

    // Pass through open files.
    let params = context.params.clone();

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

    // Get the actual execution result from the body of the function.
    let result = result?;

    // Report back the exit code, and honor any requests to exit the whole shell.
    Ok(if result.exit_shell {
        CommandSpawnResult::ExitShell(result.exit_code)
    } else {
        CommandSpawnResult::ImmediateExit(result.exit_code)
    })
}

pub(crate) async fn invoke_command_in_subshell_and_get_output(
    shell: &mut Shell,
    params: &ExecutionParameters,
    s: String,
) -> Result<String, error::Error> {
    // Instantiate a subshell to run the command in.
    let mut subshell = shell.clone();

    // Get our own set of parameters we can customize and use.
    let mut params = params.clone();
    params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    // Set up pipe so we can read the output.
    let (reader, writer) = sys::pipes::pipe()?;
    params
        .open_files
        .files
        .insert(1, openfiles::OpenFile::PipeWriter(writer));

    // Run the command.
    let result = subshell.run_string(s, &params).await?;

    // Make sure the subshell and params are closed; among other things, this
    // ensures they're not holding onto the write end of the pipe.
    drop(subshell);
    drop(params);

    // Store the status.
    shell.last_exit_status = result.exit_code;

    // Extract output.
    let output_str = std::io::read_to_string(reader)?;

    Ok(output_str)
}
