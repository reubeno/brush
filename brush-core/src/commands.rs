//! Command execution

use std::{
    borrow::Cow,
    ffi::OsStr,
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
};

use brush_parser::ast;
use itertools::Itertools;
use sys::commands::{CommandExt, CommandFdInjectionExt, CommandFgControlExt};

use crate::{
    ErrorKind, ExecutionControlFlow, ExecutionParameters, ExecutionResult, Shell, ShellFd,
    builtins, commands, env, error, escape, functions,
    interp::{self, Execute, ProcessGroupPolicy},
    openfiles::{self, OpenFile, OpenFiles},
    pathsearch, processes,
    results::ExecutionSpawnResult,
    sys, trace_categories, traps, variables,
};

/// Encapsulates the result of waiting for a command to complete.
pub enum CommandWaitResult {
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
        self.params.stdin(self.shell)
    }

    /// Returns the standard output file; usable with `write!` et al.
    pub fn stdout(&self) -> impl std::io::Write + 'static {
        self.params.stdout(self.shell)
    }

    /// Returns the standard error file; usable with `write!` et al.
    pub fn stderr(&self) -> impl std::io::Write + 'static {
        self.params.stderr(self.shell)
    }

    /// Returns the file descriptor with the given number. Returns `None`
    /// if the file descriptor is not open.
    ///
    /// # Arguments
    ///
    /// * `fd` - The file descriptor number to retrieve.
    pub fn try_fd(&self, fd: ShellFd) -> Option<openfiles::OpenFile> {
        self.params.try_fd(self.shell, fd)
    }

    /// Iterates over all open file descriptors.
    pub fn iter_fds(&self) -> impl Iterator<Item = (ShellFd, openfiles::OpenFile)> {
        self.params.iter_fds(self.shell)
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

/// Encapsulates a possibly-owned reference to a `Shell` for command execution.
pub enum ShellForCommand<'a> {
    /// The command is run in the same shell as its parent; the provided
    /// mutable reference allows modifying the parent shell.
    ParentShell(&'a mut Shell),
    /// The command is run in its own owned shell (which is also provided).
    OwnedShell {
        /// The owned shell.
        target: Box<Shell>,
        /// The parent shell.
        parent: &'a mut Shell,
    },
}

impl std::ops::Deref for ShellForCommand<'_> {
    type Target = Shell;

    fn deref(&self) -> &Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

impl std::ops::DerefMut for ShellForCommand<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

/// Composes a `std::process::Command` to execute the given command. Appropriately
/// configures the command name and arguments, redirections, injected file
/// descriptors, environment variables, etc.
///
/// # Arguments
///
/// * `context` - The execution context in which the command is being composed.
/// * `command_name` - The name of the command to execute.
/// * `argv0` - The value to use for `argv[0]` (may be different from the command).
/// * `args` - The arguments to pass to the command.
/// * `empty_env` - If true, the command will be executed with an empty environment; if false, the
///   command will inherit environment variables marked as exported in the provided `Shell`.
#[allow(unused_variables, reason = "argv0 is only used on unix platforms")]
pub fn compose_std_command<S: AsRef<OsStr>>(
    context: &ExecutionContext<'_>,
    command_name: &str,
    argv0: &str,
    args: &[S],
    empty_env: bool,
) -> Result<std::process::Command, error::Error> {
    let mut cmd = std::process::Command::new(command_name);

    // Override argv[0].
    // NOTE: Not supported on all platforms.
    cmd.arg0(argv0);

    // Pass through args.
    cmd.args(args);

    // Use the shell's current working dir.
    cmd.current_dir(context.shell.working_dir());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    if !empty_env {
        for (k, v) in context.shell.env.iter_exported() {
            // NOTE: To match bash behavior, we only include exported variables
            // that are set (i.e., have a value). This means a variable that
            // shows up in `declare -p` but has no *set* value will be omitted.
            if v.value().is_set() {
                cmd.env(k.as_str(), v.value().to_cow_str(context.shell).as_ref());
            }
        }
    }

    // Add in exported functions.
    if !empty_env {
        for (func_name, registration) in context.shell.funcs().iter() {
            if registration.is_exported() {
                let var_name = std::format!("BASH_FUNC_{func_name}%%");
                let value = std::format!("() {}", registration.definition().body);
                cmd.env(var_name, value);
            }
        }
    }

    // Redirect stdin, if applicable.
    match context.try_fd(OpenFiles::STDIN_FD) {
        Some(OpenFile::Stdin(_)) | None => (),
        Some(stdin_file) => {
            let as_stdio: Stdio = stdin_file.into();
            cmd.stdin(as_stdio);
        }
    }

    // Redirect stdout, if applicable.
    match context.try_fd(OpenFiles::STDOUT_FD) {
        Some(OpenFile::Stdout(_)) | None => (),
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.into();
            cmd.stdout(as_stdio);
        }
    }

    // Redirect stderr, if applicable.
    match context.try_fd(OpenFiles::STDERR_FD) {
        Some(OpenFile::Stderr(_)) | None => {}
        Some(stderr_file) => {
            let as_stdio: Stdio = stderr_file.into();
            cmd.stderr(as_stdio);
        }
    }

    // Inject any other fds.
    let other_files = context.iter_fds().filter(|(fd, _)| {
        *fd != OpenFiles::STDIN_FD && *fd != OpenFiles::STDOUT_FD && *fd != OpenFiles::STDERR_FD
    });
    cmd.inject_fds(other_files)?;

    Ok(cmd)
}

pub(crate) async fn on_preexecute(
    cmd: &mut commands::SimpleCommand<'_>,
) -> Result<(), error::Error> {
    // See if we have a DEBUG trap handler registered; call it if we do.
    invoke_debug_trap_handler_if_registered(&mut cmd.shell, &cmd.params, cmd.args.as_slice())
        .await?;

    Ok(())
}

async fn invoke_debug_trap_handler_if_registered(
    shell: &mut Shell,
    params: &ExecutionParameters,
    args: &[CommandArg],
) -> Result<(), error::Error> {
    if shell.call_stack().trap_handler_depth() > 0 {
        return Ok(());
    }

    let Some(debug_trap_handler) = shell.traps.get_handler(traps::TrapSignal::Debug).cloned()
    else {
        return Ok(());
    };

    // TODO(traps): Confirm whether trap handlers should be executed in the same process
    // group.
    let mut handler_params = params.clone();
    handler_params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    let full_cmd = args.iter().map(|arg| arg.to_string()).join(" ");

    // TODO(well-known-vars): This shouldn't *just* be set in a trap situation.
    shell.env.update_or_add(
        "BASH_COMMAND",
        variables::ShellValueLiteral::Scalar(full_cmd),
        |_| Ok(()),
        env::EnvironmentLookup::Anywhere,
        env::EnvironmentScope::Global,
    )?;

    shell.enter_trap_handler(Some(&debug_trap_handler));

    // TODO(traps): Discard result?
    let _ = shell
        .run_string(
            &debug_trap_handler.command,
            &debug_trap_handler.source_info,
            &handler_params,
        )
        .await;

    shell.leave_trap_handler();

    Ok(())
}

/// Represents a simple command to be executed.
pub struct SimpleCommand<'a> {
    /// The shell to run the command in.
    shell: ShellForCommand<'a>,

    /// The execution parameters for the command.
    pub params: ExecutionParameters,

    /// The name of the command to execute.
    pub command_name: String,

    /// The arguments to the command, including the command itself.
    pub args: Vec<CommandArg>,

    /// Whether to consider shell functions when looking up the command name.
    /// If true, shell functions will be checked; if false, they will be ignored.
    pub use_functions: bool,

    /// Optional list of directories to search for external commands. If left
    /// `None`, the default search logic will be used.
    pub path_dirs: Option<Vec<String>>,

    /// The process group ID to use for externally executed commands. This may be
    /// `None`, in which case the default behavior will be used.
    pub process_group_id: Option<i32>,

    /// Optionally provides a function that can run after execution occurs. Note
    /// that it is *not* invoked if the shell is discarded during the execution
    /// process.
    #[allow(clippy::type_complexity)]
    pub post_execute: Option<fn(&mut Shell) -> Result<(), error::Error>>,
}

impl<'a> SimpleCommand<'a> {
    /// Creates a new `SimpleCommand` instance.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell in which to execute the command.
    /// * `params` - The execution parameters for the command.
    /// * `command_name` - The name of the command to execute.
    /// * `args` - The arguments to the command, including the command itself.
    pub const fn new(
        shell: ShellForCommand<'a>,
        params: ExecutionParameters,
        command_name: String,
        args: Vec<CommandArg>,
    ) -> Self {
        Self {
            shell,
            params,
            command_name,
            args,
            use_functions: true,
            path_dirs: None,
            process_group_id: None,
            post_execute: None,
        }
    }

    /// Executes the simple command.
    ///
    /// The command may be a builtin, a shell function, or an externally
    /// executed command. This function's implementation is responsible for
    /// dispatching it appropriately according to the context provided.
    pub async fn execute(mut self) -> Result<ExecutionSpawnResult, error::Error> {
        // First see if it's the name of a builtin.
        let builtin = self.shell.builtins().get(&self.command_name).cloned();

        // If we found a special builtin (that's not disabled), then invoke it.
        if builtin
            .as_ref()
            .is_some_and(|r| !r.disabled && r.special_builtin)
        {
            let builtin = builtin.unwrap();
            return self.execute_via_builtin(builtin).await;
        }

        // Assuming we weren't requested not to do so, check if it's the name of
        // a shell function.
        if self.use_functions {
            if let Some(func_registration) =
                self.shell.funcs().get(self.command_name.as_str()).cloned()
            {
                return self.execute_via_function(func_registration).await;
            }
        }

        // If we found a (non-special) builtin and it's not disabled, then invoke it.
        if let Some(builtin) = builtin {
            if !builtin.disabled {
                return self.execute_via_builtin(builtin).await;
            }
        }

        // We still haven't found a command to invoke. We'll need to look for an external command.
        if !self.command_name.contains(std::path::MAIN_SEPARATOR) {
            // All else failed; if we were given path directories to search, try to look through
            // them for a matching executable. Otherwise, use our default search logic.
            let path = if let Some(path_dirs) = &self.path_dirs {
                pathsearch::search_for_executable(
                    path_dirs.iter().map(String::as_str),
                    self.command_name.as_str(),
                )
                .next()
            } else {
                self.shell
                    .find_first_executable_in_path_using_cache(&self.command_name)
            };

            if let Some(path) = path {
                self.execute_via_external(&path)
            } else {
                if let Some(post_execute) = self.post_execute {
                    let _ = post_execute(&mut self.shell);
                }

                Err(ErrorKind::CommandNotFound(self.command_name).into())
            }
        } else {
            let command_name = PathBuf::from(self.command_name.clone());
            self.execute_via_external(command_name.as_path())
        }
    }

    async fn execute_via_builtin(
        self,
        builtin: builtins::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        match self.shell {
            ShellForCommand::OwnedShell { target, .. } => {
                Ok(Self::execute_via_builtin_in_owned_shell(
                    *target,
                    self.params,
                    builtin,
                    self.command_name,
                    self.args,
                ))
            }
            ShellForCommand::ParentShell(..) => {
                self.execute_via_builtin_in_parent_shell(builtin).await
            }
        }
    }

    fn execute_via_builtin_in_owned_shell(
        mut shell: Shell,
        params: ExecutionParameters,
        builtin: builtins::Registration,
        command_name: String,
        args: Vec<CommandArg>,
    ) -> ExecutionSpawnResult {
        let join_handle = tokio::task::spawn_blocking(move || {
            let cmd_context = ExecutionContext {
                shell: &mut shell,
                command_name,
                params,
            };

            let rt = tokio::runtime::Handle::current();
            rt.block_on(execute_builtin_command(&builtin, cmd_context, args))
        });

        ExecutionSpawnResult::StartedTask(join_handle)
    }

    async fn execute_via_builtin_in_parent_shell(
        self,
        builtin: builtins::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let result = execute_builtin_command(&builtin, cmd_context, self.args).await;

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        let result = result?;

        Ok(result.into())
    }

    async fn execute_via_function(
        self,
        func_registration: functions::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        // Strip the function name off args.
        let result = invoke_shell_function(func_registration, cmd_context, &self.args[1..]).await;

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        result
    }

    fn execute_via_external(self, path: &Path) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let resolved_path = path.to_string_lossy();
        let result = execute_external_command(
            cmd_context,
            resolved_path.as_ref(),
            self.process_group_id,
            &self.args[1..],
        );

        if let Some(post_execute) = self.post_execute {
            let _ = post_execute(&mut shell);
        }

        result
    }
}

pub(crate) fn execute_external_command(
    context: ExecutionContext<'_>,
    executable_path: &str,
    process_group_id: Option<i32>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    // Filter out the args; we only want strings.
    let mut cmd_args = vec![];
    for arg in args {
        if let CommandArg::String(s) = arg {
            cmd_args.push(s);
        }
    }

    // Before we lose ownership of the open files, figure out if stdin will be a terminal.
    let child_stdin_is_terminal = context
        .try_fd(openfiles::OpenFiles::STDIN_FD)
        .is_some_and(|f| f.is_terminal());

    // Figure out if we should be setting up a new process group.
    let new_pg = context.should_cmd_lead_own_process_group();

    // Compose the std::process::Command that encapsulates what we want to launch.
    #[allow(unused_mut, reason = "only mutated on unix platforms")]
    let mut cmd = compose_std_command(
        &context,
        executable_path,
        context.command_name.as_str(),
        cmd_args.as_slice(),
        false, /* empty environment? */
    )?;

    // Set up process group state.
    if new_pg {
        // We need to set up a new process group.
        cmd.process_group(0);
    } else {
        // We need to join an established process group.
        if let Some(pgid) = process_group_id {
            cmd.process_group(pgid);
        }
    }

    // If we're to lead our own process group and stdin is a terminal,
    // then we need to arrange for the new process to move itself
    // to the foreground.
    if new_pg && child_stdin_is_terminal {
        cmd.take_foreground();
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
            let mut actual_pgid = process_group_id;
            if let Some(pid) = &pid {
                if new_pg {
                    actual_pgid = Some(*pid);
                }
            } else {
                tracing::warn!("could not retrieve pid for child process");
            }

            Ok(ExecutionSpawnResult::StartedProcess(
                processes::ChildProcess::new(child, pid, actual_pgid),
            ))
        }
        Err(spawn_err) => {
            if context.shell.options.interactive {
                sys::terminal::move_self_to_foreground()?;
            }

            if spawn_err.kind() == std::io::ErrorKind::NotFound {
                if !context.shell.working_dir().exists() {
                    Err(
                        error::ErrorKind::WorkingDirMissing(context.shell.working_dir().to_owned())
                            .into(),
                    )
                } else {
                    Err(error::ErrorKind::CommandNotFound(context.command_name).into())
                }
            } else {
                Err(
                    error::ErrorKind::FailedToExecuteCommand(context.command_name, spawn_err)
                        .into(),
                )
            }
        }
    }
}

async fn execute_builtin_command(
    builtin: &builtins::Registration,
    context: ExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> Result<ExecutionResult, error::Error> {
    match &builtin.execute_func {
        builtins::CommandExecuteFunc::Async(async_func) => async_func(context, args).await,
        builtins::CommandExecuteFunc::Sync(sync_func) => sync_func(context, args),
    }
}

pub(crate) async fn invoke_shell_function(
    function: functions::Registration,
    mut context: ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    let ast::FunctionBody(body, redirects) = &function.definition().body;

    // Apply any redirects specified at function definition-time.
    if let Some(redirects) = redirects {
        for redirect in &redirects.0 {
            interp::setup_redirect(context.shell, &mut context.params, redirect).await?;
        }
    }

    let positional_args = args.iter().map(|a| a.to_string());

    // Pass through open files.
    let params = context.params.clone();

    // Note that we're going deeper. Once we do this, we need to make sure we don't bail early
    // before "exiting" the function.
    context.shell.enter_function(
        context.command_name.as_str(),
        &function,
        positional_args,
        &context.params,
    )?;

    // Invoke the function.
    let result = body.execute(context.shell, &params).await;

    // Clean up parameters so any owned files are closed.
    drop(params);

    // We've come back out, reflect it.
    context.shell.leave_function()?;

    // Get the actual execution result from the body of the function.
    let mut result = result?;

    // Handle control-flow.
    match result.next_control_flow {
        ExecutionControlFlow::BreakLoop { .. } | ExecutionControlFlow::ContinueLoop { .. } => {
            return error::unimp("break or continue returned from function invocation");
        }
        ExecutionControlFlow::ReturnFromFunctionOrScript => {
            // It's now been handled.
            result.next_control_flow = ExecutionControlFlow::Normal;
        }
        _ => {}
    }

    Ok(result.into())
}

pub(crate) async fn invoke_command_in_subshell_and_get_output(
    shell: &mut Shell,
    params: &ExecutionParameters,
    s: String,
) -> Result<String, error::Error> {
    // Instantiate a subshell to run the command in.
    let mut subshell = shell.clone();

    // Command substitutions don't inherit errexit by default. Only inherit it when
    // command_subst_inherits_errexit is enabled, otherwise disable errexit in the subshell.
    if !shell.options.command_subst_inherits_errexit {
        subshell.options.exit_on_nonzero_command_exit = false;
    }

    // Get our own set of parameters we can customize and use.
    let mut params = params.clone();
    params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    // Set up pipe so we can read the output.
    let (reader, writer) = std::io::pipe()?;
    params.set_fd(OpenFiles::STDOUT_FD, writer.into());

    // Start the execution of the command, but don't wait for it to
    // complete. In case the command generates lots of output, we
    // need to start reading in parallel so the command doesn't block
    // when the pipe's buffer fills up. We pass ownership of the
    // subshell and params to run_substitution_command; we must
    // ensure that they're both dropped by the time this call
    // returns (so they're not holding onto the write end of the pipe).
    let cmd_join_handle = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(run_substitution_command(subshell, params, s))
    });

    // Extract output.
    let output_str = std::io::read_to_string(reader)?;

    // Now observe the command's completion.
    let run_result = cmd_join_handle.await?;
    let cmd_result = run_result?;

    // Store the status.
    shell.set_last_exit_status(cmd_result.exit_code.into());

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
            std::io::copy(&mut params.stdin(&shell), &mut params.stdout(&shell))?;
            return Ok(ExecutionResult::new(0));
        }
    }

    // TODO(source-info): review this
    let source_info = crate::SourceInfo::from("main");

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
