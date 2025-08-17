use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{borrow::Cow, ffi::OsStr, fmt::Display, process::Stdio, sync::Arc};

use brush_parser::ast;
#[cfg(unix)]
use command_fds::{CommandFdExt, FdMapping};
use itertools::Itertools;

use crate::{
    ExecutionParameters, ExecutionResult, Shell, builtins, env, error, escape,
    interp::{self, Execute, ProcessGroupPolicy},
    openfiles::{self, OpenFile, OpenFiles},
    pathsearch, processes, sys, trace_categories, traps, variables,
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
    pub async fn wait(self, no_wait: bool) -> Result<CommandWaitResult, error::Error> {
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

#[allow(unused_variables, reason = "argv0 is only used on unix platforms")]
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
    match open_files.remove(OpenFiles::STDIN_FD) {
        Some(OpenFile::Stdin) | None => (),
        Some(stdin_file) => {
            let as_stdio: Stdio = stdin_file.into();
            cmd.stdin(as_stdio);
        }
    }

    // Redirect stdout, if applicable.
    match open_files.remove(OpenFiles::STDOUT_FD) {
        Some(OpenFile::Stdout) | None => (),
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.into();
            cmd.stdout(as_stdio);
        }
    }

    // Redirect stderr, if applicable.
    match open_files.remove(OpenFiles::STDERR_FD) {
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
        if !open_files.is_empty() {
            return error::unimp("fd redirections on non-Unix platform");
        }
    }

    Ok(cmd)
}

pub(crate) async fn on_preexecute(
    context: &mut ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<(), error::Error> {
    // See if we have a DEBUG trap handler registered; call it if we do.
    invoke_debug_trap_handler_if_registered(context, args).await?;

    Ok(())
}

async fn invoke_debug_trap_handler_if_registered(
    context: &mut ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<(), error::Error> {
    if context.shell.traps.handler_depth == 0 {
        let debug_trap_handler = context
            .shell
            .traps
            .handlers
            .get(&traps::TrapSignal::Debug)
            .cloned();
        if let Some(debug_trap_handler) = debug_trap_handler {
            // TODO: Confirm whether trap handlers should be executed in the same process
            // group.
            let handler_params = ExecutionParameters {
                open_files: context.params.open_files.clone(),
                process_group_policy: ProcessGroupPolicy::SameProcessGroup,
            };

            let full_cmd = args.iter().map(|arg| arg.to_string()).join(" ");

            // TODO: This shouldn't *just* be set in a trap situation.
            context.shell.env.update_or_add(
                "BASH_COMMAND",
                variables::ShellValueLiteral::Scalar(full_cmd),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;

            context.shell.traps.handler_depth += 1;

            // TODO: Discard result?
            let _ = context
                .shell
                .run_string(debug_trap_handler, &handler_params)
                .await?;

            context.shell.traps.handler_depth -= 1;
        }
    }

    Ok(())
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
            pathsearch::search_for_executable(
                path_dirs.iter().map(String::as_str),
                cmd_context.command_name.as_str(),
            )
            .next()
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
    #[cfg(unix)]
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
    #[allow(unused_mut, reason = "only mutated on unix platforms")]
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
    } else {
        // We need to join an established process group.
        #[cfg(unix)]
        if let Some(pgid) = process_group_id {
            cmd.process_group(*pgid);
        }
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
            #[expect(clippy::cast_possible_wrap)]
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
    let subshell = shell.clone();

    // Get our own set of parameters we can customize and use.
    let mut params = params.clone();
    params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    // Set up pipe so we can read the output.
    let (reader, writer) = openfiles::pipe()?;
    params.open_files.set(OpenFiles::STDOUT_FD, writer.into());

    // Run the command. Pass ownership of the subshell and params to
    // run_substitution_command; we must ensure that they're both dropped
    // by the time the call returns (so they're not holding onto the write
    // end of the pipe).
    let result = run_substitution_command(subshell, params, s).await?;

    // Store the status.
    shell.last_exit_status = result.exit_code;

    // Extract output.
    let output_str = std::io::read_to_string(OpenFile::from(reader))?;

    Ok(output_str)
}

async fn run_substitution_command(
    mut shell: Shell,
    mut params: ExecutionParameters,
    command: String,
) -> Result<ExecutionResult, error::Error> {
    // Parse the string into a whole shell program.
    let parse_result = shell.parse_string(command);

    // Check for a command that is only an input redirection ("< file").
    // If detected, emulate `cat file` to stdout and return immediately.
    // If we failed to parse, then we'll fall below and handle it there.
    if let Ok(program) = &parse_result {
        if let Some(redir) = try_unwrap_bare_input_redir_program(program) {
            interp::setup_redirect(&mut shell, &mut params, redir).await?;
            std::io::copy(&mut params.stdin(), &mut params.stdout())?;
            return Ok(ExecutionResult::new(0));
        }
    }

    let source_info = brush_parser::SourceInfo {
        source: String::from("main"),
    };

    // Handle the parse result using default shell behavior.
    shell
        .run_parsed_result(parse_result, &source_info, &params)
        .await
}

// Detects a subshell command that consists solely of a single input redirection
// (e.g., "< file"), returning the IoRedirect when present.
fn try_unwrap_bare_input_redir_program(program: &ast::Program) -> Option<&ast::IoRedirect> {
    // We're looking for exactly one complete command...
    let [complete] = program.complete_commands.as_slice() else {
        return None;
    };

    // ...a single list item...
    let ast::CompoundList(items) = complete;
    let [item] = items.as_slice() else {
        return None;
    };

    // ...with a single pipeline (no && or || chaining)...
    let and_or = &item.0;
    if !and_or.additional.is_empty() {
        return None;
    }

    // ...not negated...
    let pipeline = &and_or.first;
    if pipeline.bang {
        return None;
    }

    // ...with a single command in the pipeline...
    let [ast::Command::Simple(simple_cmd)] = pipeline.seq.as_slice() else {
        return None;
    };

    // ...with no program word/name and no suffix...
    if simple_cmd.word_or_name.is_some() || simple_cmd.suffix.is_some() {
        return None;
    }

    // ...and exactly one prefix containing an I/O redirect...
    let prefix = simple_cmd.prefix.as_ref()?;
    let [ast::CommandPrefixOrSuffixItem::IoRedirect(redir)] = prefix.0.as_slice() else {
        return None;
    };

    // ...that is a file input redirection to a filename, targeting stdin.
    match redir {
        ast::IoRedirect::File(
            fd,
            ast::IoFileRedirectKind::Read,
            ast::IoFileRedirectTarget::Filename(..),
        ) if fd.is_none_or(|fd| fd == openfiles::OpenFiles::STDIN_FD) => Some(redir),
        _ => None,
    }
}
