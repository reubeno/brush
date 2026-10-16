//! Command execution

use std::{
    borrow::Cow,
    fmt::{Display, Write as _},
    path::{Path, PathBuf},
};

use brush_parser::ast;

use crate::{
    ErrorKind, ExecutionControlFlow, ExecutionExitCode, ExecutionParameters, ExecutionResult,
    Shell, ShellFd, builtins, commands, env, error, escape,
    extensions::{self, ProcessLauncher as _, ShellExtensions},
    functions,
    interp::{self, Execute, ProcessGroupPolicy},
    openfiles::{self, OpenFiles},
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
pub struct ExecutionContext<'a, SE: ShellExtensions = extensions::DefaultShellExtensions> {
    /// The shell in which the command is being executed.
    pub shell: &'a mut Shell<SE>,
    /// The name of the command being executed. A failure to start a program as the
    /// command (see [`Self::spawn_program`]) is reported by this name.
    pub command_name: String,
    /// The parameters for the execution.
    pub params: ExecutionParameters,
}

impl<SE: ShellExtensions> ExecutionContext<'_, SE> {
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

    /// Composes the [`processes::LaunchRequest`] for a program to run as the
    /// command: with the given arguments, the shell's exported variables and
    /// functions as its environment, the shell's working directory, the command's
    /// open files, and the shell's process group. A caller may change any of that
    /// before starting the program, with [`Self::spawn_program`] or
    /// [`Self::exec_program`].
    ///
    /// # Arguments
    ///
    /// * `program` - The path of the program (e.g., as
    ///   [`Shell::resolve_external_command`] resolves a name to it).
    /// * `argv0` - The value the program should see as `argv[0]`.
    /// * `args` - The arguments to the program, not including `argv[0]`.
    pub fn compose_launch_request<S: AsRef<str>>(
        &self,
        program: &Path,
        argv0: &str,
        args: &[S],
    ) -> processes::LaunchRequest {
        processes::LaunchRequest {
            program: program.to_owned(),
            argv0: argv0.to_owned(),
            args: args.iter().map(|arg| arg.as_ref().to_owned()).collect(),
            env: compose_env(self.shell),
            working_dir: self.shell.working_dir().to_owned(),
            open_files: self.iter_fds().into(),
            process_group: processes::ProcessGroup::Inherit,
            kill_on_drop: self.shell.options().kill_external_commands_on_drop,
        }
    }

    /// Starts the process described by `request` as the command, as the shell
    /// starts any external command: with the shell's process launcher, in the
    /// process group the command's process-group policy calls for (which replaces
    /// the request's). A failure to start it is reported as running the command
    /// would report it, by [`Self::command_name`]. The caller waits for the process,
    /// through the result.
    ///
    /// # Arguments
    ///
    /// * `request` - The process to start (e.g., from [`Self::compose_launch_request`]).
    pub fn spawn_program(
        &self,
        mut request: processes::LaunchRequest,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        request.process_group = self.process_group_for(&request);

        tracing::debug!(
            target: trace_categories::COMMANDS,
            "Spawning: cmd='{} {}'",
            request.program.display(),
            request.args.join(" ")
        );

        let working_dir = request.working_dir.clone();
        match self.shell.process_launcher().spawn(request) {
            Ok(child) => Ok(ExecutionSpawnResult::StartedProcess(child)),
            Err(err) => {
                if self.shell.options().interactive {
                    sys::terminal::move_self_to_foreground()?;
                }

                Err(self.launch_error(err, &working_dir))
            }
        }
    }

    /// Replaces the shell's process with the process described by `request`, with
    /// the shell's process launcher, as `exec` does. Returns only if that fails, with
    /// the failure reported as running the command would report it, by
    /// [`Self::command_name`].
    ///
    /// # Arguments
    ///
    /// * `request` - The process to replace the shell's with.
    pub fn exec_program(&self, request: processes::LaunchRequest) -> error::Error {
        let working_dir = request.working_dir.clone();
        let err = self.shell.process_launcher().exec(request);
        self.launch_error(err, &working_dir)
    }

    /// Returns the process group that a process started as the command should run
    /// in, under the command's process-group policy.
    fn process_group_for(&self, request: &processes::LaunchRequest) -> processes::ProcessGroup {
        if matches!(
            self.params.process_group_policy,
            ProcessGroupPolicy::NewProcessGroup
        ) {
            let child_stdin_is_terminal = request
                .open_files
                .try_fd(openfiles::OpenFiles::STDIN_FD)
                .is_some_and(|f| f.is_terminal());

            if child_stdin_is_terminal && self.shell.options().external_cmd_leads_session {
                processes::ProcessGroup::NewSession
            } else {
                processes::ProcessGroup::New {
                    take_foreground: child_stdin_is_terminal,
                }
            }
        } else if let Some(pgid) = self.params.process_group_id {
            // We need to join an established process group.
            processes::ProcessGroup::Join(pgid)
        } else {
            processes::ProcessGroup::Inherit
        }
    }

    /// Translates an error from starting a process as the command into the shell's
    /// error for it.
    fn launch_error(&self, err: std::io::Error, working_dir: &Path) -> error::Error {
        if err.kind() == std::io::ErrorKind::NotFound {
            if !working_dir.exists() {
                error::ErrorKind::WorkingDirMissing(working_dir.to_owned()).into()
            } else {
                error::ErrorKind::CommandNotFound(self.command_name.clone()).into()
            }
        } else {
            error::ErrorKind::FailedToExecuteCommand(self.command_name.clone(), err).into()
        }
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
    const fn to_string_len_hint(&self) -> usize {
        match self {
            Self::String(s) => s.len(),
            Self::Assignment(_) => 0,
        }
    }

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
pub enum ShellForCommand<'a, SE: extensions::ShellExtensions> {
    /// The command is run in the same shell as its parent; the provided
    /// mutable reference allows modifying the parent shell.
    ParentShell(&'a mut Shell<SE>),
    /// The command is run in its own owned shell (which is also provided).
    OwnedShell {
        /// The owned shell.
        target: Box<Shell<SE>>,
        /// The parent shell.
        parent: &'a mut Shell<SE>,
    },
}

impl<SE: extensions::ShellExtensions> std::ops::Deref for ShellForCommand<'_, SE> {
    type Target = Shell<SE>;

    fn deref(&self) -> &Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

impl<SE: extensions::ShellExtensions> std::ops::DerefMut for ShellForCommand<'_, SE> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ShellForCommand::ParentShell(shell) => shell,
            ShellForCommand::OwnedShell { target, .. } => target,
        }
    }
}

/// Composes the environment for an external program: the shell's exported
/// variables and functions.
///
/// # Arguments
///
/// * `shell` - The shell whose exported state should be used.
fn compose_env(shell: &Shell<impl extensions::ShellExtensions>) -> Vec<(String, String)> {
    let mut env = vec![];

    // Add in exported variables.
    for (k, v) in shell.env().iter_exported() {
        // NOTE: To match bash behavior, we only include exported variables
        // that are set (i.e., have a value). This means a variable that
        // shows up in `declare -p` but has no *set* value will be omitted.
        if v.value().is_set() {
            env.push((k.clone(), v.value().to_cow_str(shell).into_owned()));
        }
    }

    // Add in exported functions.
    for (func_name, registration) in shell.funcs().iter() {
        if registration.is_exported() {
            let var_name = std::format!("BASH_FUNC_{func_name}%%");
            let value = std::format!("() {}", registration.definition().body);
            env.push((var_name, value));
        }
    }

    env
}

pub(crate) async fn on_preexecute(
    cmd: &mut commands::SimpleCommand<'_, impl extensions::ShellExtensions>,
) -> Result<(), error::Error> {
    // Set BASH_COMMAND before invoking the DEBUG trap (and generally before
    // executing commands).
    let full_cmd = format_command_args(&cmd.args);
    cmd.shell.env_mut().update_or_add(
        "BASH_COMMAND",
        variables::ShellValueLiteral::Scalar(full_cmd),
        |_| Ok(()),
        env::EnvironmentLookup::Anywhere,
        env::EnvironmentScope::Global,
    )?;

    // Fire the DEBUG trap if one is registered.
    if cmd.shell.traps().handles(traps::TrapSignal::Debug) {
        let _ = cmd
            .shell
            .invoke_trap_handler(traps::TrapSignal::Debug, &cmd.params)
            .await?;
    }

    Ok(())
}

fn format_command_args(args: &[CommandArg]) -> String {
    let capacity = args
        .iter()
        .map(CommandArg::to_string_len_hint)
        .sum::<usize>()
        + args.len().saturating_sub(1);
    let mut result = String::with_capacity(capacity);

    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            result.push(' ');
        }
        let _ = write!(result, "{arg}");
    }

    result
}

/// Represents a simple command to be executed.
pub struct SimpleCommand<'a, SE: extensions::ShellExtensions> {
    /// The shell to run the command in.
    shell: ShellForCommand<'a, SE>,

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
    pub path_dirs: Option<Vec<PathBuf>>,

    /// Optional override for the `argv[0]` value presented to an externally
    /// spawned process. When `None`, `command_name` is used.
    pub argv0: Option<String>,
}

impl<'a, SE: extensions::ShellExtensions> SimpleCommand<'a, SE> {
    /// Creates a new `SimpleCommand` instance.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell in which to execute the command.
    /// * `params` - The execution parameters for the command.
    /// * `command_name` - The name of the command to execute.
    /// * `args` - The arguments to the command, including the command itself.
    pub fn new<I>(
        shell: ShellForCommand<'a, SE>,
        params: ExecutionParameters,
        command_name: String,
        args: I,
    ) -> Self
    where
        I: IntoIterator<Item = CommandArg>,
    {
        Self {
            shell,
            params,
            command_name,
            args: args.into_iter().collect(),
            use_functions: true,
            path_dirs: None,
            argv0: None,
        }
    }

    /// Executes the simple command.
    ///
    /// The command may be a builtin, a shell function, or an externally
    /// executed command. This function's implementation is responsible for
    /// dispatching it appropriately according to the context provided.
    #[allow(
        clippy::missing_panics_doc,
        reason = "these unwrap calls should not panic"
    )]
    pub async fn execute(mut self) -> Result<ExecutionSpawnResult, error::Error> {
        // First see if it's the name of a builtin.
        let builtin = self.shell.builtins().get(&self.command_name).cloned();

        // If we're in POSIX mode and found a special builtin (that's not disabled), then invoke it
        // without considering functions.
        if self.shell.options().posix_mode
            && builtin
                .as_ref()
                .is_some_and(|r| !r.disabled && r.special_builtin)
        {
            #[allow(clippy::unwrap_used, reason = "we just checked that builtin is Some")]
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

        // If we haven't yet resolved the command name and found a builtin that's not disabled,
        // then invoke it.
        if let Some(builtin) = builtin {
            if !builtin.disabled {
                return self.execute_via_builtin(builtin).await;
            }
        }

        // We still haven't found a command to invoke. We'll need to look for an external command.
        // If we were given path directories to search, look through them for a match.
        // Otherwise, use our default search logic.
        let path = match &self.path_dirs {
            Some(path_dirs) if !sys::fs::contains_path_separator(&self.command_name) => {
                pathsearch::resolve_command(path_dirs, self.command_name.as_str())
            }
            _ => self.shell.resolve_external_command(&self.command_name),
        };

        if let Some(path) = path {
            self.execute_via_external(&path)
        } else {
            // Bash updates $_ even when the command is not found, so mirror
            // that here before reporting the error.
            let last_arg = Self::take_last_arg(&self.args);
            self.shell.update_last_arg_variable(last_arg);

            Err(ErrorKind::CommandNotFound(self.command_name).into())
        }
    }

    /// Extracts the owned string representation of the last argument of a
    /// command, suitable for recording into `$_`.
    fn take_last_arg(args: &[CommandArg]) -> Option<String> {
        args.last().map(ToString::to_string)
    }

    async fn execute_via_builtin(
        self,
        builtin: builtins::Registration<SE>,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        match self.shell {
            ShellForCommand::OwnedShell { target, .. } => {
                Ok(Self::execute_via_builtin_in_owned_shell(
                    *target,
                    self.params,
                    builtin,
                    self.command_name,
                    self.args,
                )
                .await)
            }
            ShellForCommand::ParentShell(..) => {
                self.execute_via_builtin_in_parent_shell(builtin).await
            }
        }
    }

    // Not an `async fn`: its future would hold `shell`, and that would grow the
    // future of every simple command, not just this rare case.
    fn execute_via_builtin_in_owned_shell(
        mut shell: Shell<SE>,
        params: ExecutionParameters,
        builtin: builtins::Registration<SE>,
        command_name: String,
        args: Vec<CommandArg>,
    ) -> impl Future<Output = ExecutionSpawnResult> + use<SE> {
        let last_arg = Self::take_last_arg(&args);
        // Builtins may block (e.g., reading input), so keep them from holding up the
        // rest of the pipeline.
        let task = sys::tasks::spawn_blocking(async move {
            let cmd_context = ExecutionContext {
                shell: &mut shell,
                command_name,
                params,
            };

            let result = execute_builtin_command(&builtin, cmd_context, args).await;

            // Update $_ after command execution.
            shell.update_last_arg_variable(last_arg);

            result
        });

        async move { ExecutionSpawnResult::StartedTask(task.await) }
    }

    async fn execute_via_builtin_in_parent_shell(
        self,
        builtin: builtins::Registration<SE>,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;
        let last_arg = Self::take_last_arg(&self.args);

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let result = execute_builtin_command(&builtin, cmd_context, self.args).await;

        // Update $_ after command execution.
        shell.update_last_arg_variable(last_arg);

        let result = result?;

        Ok(result.into())
    }

    async fn execute_via_function(
        self,
        func_registration: functions::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        match self.shell {
            ShellForCommand::OwnedShell { target, .. } => {
                Ok(Self::execute_via_function_in_owned_shell(
                    *target,
                    self.params,
                    func_registration,
                    self.command_name,
                    self.args,
                )
                .await)
            }
            ShellForCommand::ParentShell(..) => {
                self.execute_via_function_in_parent_shell(func_registration)
                    .await
            }
        }
    }

    // As a non-last pipeline stage, a function must run as a background task
    // (mirroring `execute_via_builtin_in_owned_shell` above) rather than
    // being awaited inline in `spawn_pipeline_processes`'s own per-stage
    // loop: awaiting it inline there blocks that loop until the function's
    // body fully returns, so the *next* pipeline stage (the one that would
    // actually drain this function's stdout pipe) is never even spawned
    // until this one is done. A function that writes more to its stdout
    // pipe than the OS pipe buffer holds (~64KiB on Linux) before returning
    // then blocks on that `write()` forever, since nothing is reading the
    // other end yet -- a plain deadlock, not a slow completion. Confirmed
    // empirically with a two-line repro having nothing to do with any
    // external command or builtin: `f() { for i in $(seq 1 5000); do echo
    // line $i; done; }; f | cat` hangs indefinitely. Real bash always runs
    // every pipeline stage as its own concurrently-running process, so this
    // case never arises there.
    fn execute_via_function_in_owned_shell(
        mut shell: Shell<SE>,
        params: ExecutionParameters,
        func_registration: functions::Registration,
        command_name: String,
        args: Vec<CommandArg>,
    ) -> impl Future<Output = ExecutionSpawnResult> + use<SE> {
        let last_arg = Self::take_last_arg(&args);
        let task = sys::tasks::spawn_blocking(async move {
            let cmd_context = ExecutionContext {
                shell: &mut shell,
                command_name,
                params,
            };

            let spawn_result =
                invoke_shell_function(func_registration, cmd_context, &args[1..]).await;

            // Update $_ after command execution.
            shell.update_last_arg_variable(last_arg);

            match spawn_result? {
                ExecutionSpawnResult::Completed(result) => Ok(result),
                ExecutionSpawnResult::StartedProcess(_) | ExecutionSpawnResult::StartedTask(_) => {
                    // invoke_shell_function() only ever returns Completed
                    // today; this guards against a silent behavior change
                    // rather than dropping already-started work.
                    error::unimp("invoke_shell_function returned an unexpected spawn result")
                }
            }
        });

        async move { ExecutionSpawnResult::StartedTask(task.await) }
    }

    async fn execute_via_function_in_parent_shell(
        self,
        func_registration: functions::Registration,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;
        let last_arg = Self::take_last_arg(&self.args);

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        // Strip the function name off args.
        let result = invoke_shell_function(func_registration, cmd_context, &self.args[1..]).await;

        // $_ is reset *after* the function body runs, to the last argument of
        // the invocation (or the function name itself if zero args). Any
        // mutations made inside the body are overwritten — this matches bash,
        // where the caller observes only the invocation's last argument.
        shell.update_last_arg_variable(last_arg);

        result
    }

    fn execute_via_external(self, path: &Path) -> Result<ExecutionSpawnResult, error::Error> {
        let mut shell = self.shell;
        let last_arg = Self::take_last_arg(&self.args);

        let cmd_context = ExecutionContext {
            shell: &mut shell,
            command_name: self.command_name,
            params: self.params,
        };

        let result =
            execute_external_command(&cmd_context, path, self.argv0.as_deref(), &self.args[1..]);

        // Update $_ after command execution.
        shell.update_last_arg_variable(last_arg);

        result
    }
}

pub(crate) fn execute_external_command(
    context: &ExecutionContext<'_, impl extensions::ShellExtensions>,
    executable_path: &Path,
    argv0_override: Option<&str>,
    args: &[CommandArg],
) -> Result<ExecutionSpawnResult, error::Error> {
    // Filter out the args; we only want strings.
    let cmd_args = args
        .iter()
        .filter_map(|e| {
            if let CommandArg::String(s) = e {
                Some(s)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // argv[0] defaults to context.command_name (the user-facing name of the
    // command) unless the caller specified an explicit override.
    let argv0 = argv0_override.unwrap_or(context.command_name.as_str());

    let mut request = context.compose_launch_request(executable_path, argv0, cmd_args.as_slice());

    // As in bash, a command sees `_` as the program's path, whatever the shell exports.
    set_env_var(
        &mut request.env,
        "_",
        executable_path.to_string_lossy().into_owned(),
    );

    context.spawn_program(request)
}

/// Sets `name` to `value` in `env`, in place of any value it has there.
fn set_env_var(env: &mut Vec<(String, String)>, name: &str, value: String) {
    match env.iter_mut().find(|(n, _)| n == name) {
        Some((_, v)) => *v = value,
        None => env.push((name.to_owned(), value)),
    }
}

async fn execute_builtin_command<SE: extensions::ShellExtensions>(
    builtin: &builtins::Registration<SE>,
    context: ExecutionContext<'_, SE>,
    args: Vec<CommandArg>,
) -> Result<ExecutionResult, error::Error> {
    // In POSIX mode, special builtins that return errors are to be treated as fatal.
    let mark_errors_fatal = builtin.special_builtin && context.shell.options().posix_mode;

    match (builtin.execute_func)(context, args).await {
        Ok(result) => Ok(result),
        Err(e) => {
            // Broken pipe errors should silently return the appropriate exit code
            if let Some(io_err) = e.as_io_error() {
                if io_err.kind() == std::io::ErrorKind::BrokenPipe {
                    return Ok(ExecutionExitCode::from(io_err).into());
                }
            }

            Err(if mark_errors_fatal { e.into_fatal() } else { e })
        }
    }
}

pub(crate) async fn invoke_shell_function(
    function: functions::Registration,
    mut context: ExecutionContext<'_, impl extensions::ShellExtensions>,
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

    // Enter the function; it's left however its body ends, even if it's cancelled.
    let mut shell =
        context
            .shell
            .enter_function(context.command_name.as_str(), &function, positional_args)?;

    // A function executes within the current shell process and shares its caller's open files,
    // so the parameters are passed through by shared reference rather than cloned. This prevents
    // direct mutation of the caller's `ExecutionParameters` open-file table, though the function
    // may still change the shell's persistent open files via builtins (e.g. `exec`).
    let result = body.execute(&mut shell, &context.params).await;

    // Leave the function: pop its frame and its local variables.
    drop(shell);

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
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    s: String,
) -> Result<String, error::Error> {
    // Instantiate a subshell to run the command in.
    let mut subshell = shell.clone();

    // Command substitutions don't inherit errexit by default. Only inherit it when
    // command_subst_inherits_errexit is enabled, otherwise disable errexit in the subshell.
    if !shell.options().command_subst_inherits_errexit {
        subshell.options_mut().exit_on_nonzero_command_exit = false;
    }

    // Get our own set of parameters we can customize and use.
    let mut params = params.clone();
    params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    // Set up pipe so we can read the output.
    let (reader, writer) = sys::pipes::pipe()?;
    params.set_fd(OpenFiles::STDOUT_FD, writer.into());

    let mut async_reader = sys::async_pipe::AsyncPipeReader::new(reader)?;

    let cmd_join_handle = sys::tasks::spawn(run_substitution_command(subshell, params, s));

    let output_str = async_reader.read_to_string().await?;

    // Now observe the command's completion.
    let run_result = cmd_join_handle.await?;
    let cmd_result = run_result?;

    // Store the status.
    shell.set_last_exit_status(cmd_result.exit_code.into());

    // Note: $_ is naturally isolated from the parent because we cloned the
    // shell to run the substitution.

    Ok(output_str)
}

async fn run_substitution_command(
    mut shell: Shell<impl extensions::ShellExtensions>,
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
