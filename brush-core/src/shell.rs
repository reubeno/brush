use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use normalize_path::NormalizePath;
use tokio::sync::Mutex;

use crate::arithmetic::Evaluatable;
use crate::env::{EnvironmentLookup, EnvironmentScope, ShellEnvironment};
use crate::interp::{self, Execute, ExecutionParameters};
use crate::options::RuntimeOptions;
use crate::results::ExecutionSpawnResult;
use crate::sys::fs::PathExt;
use crate::variables::{self, ShellVariable};
use crate::{
    ExecutionControlFlow, ExecutionExitCode, ExecutionResult, ProcessGroupPolicy, history,
    interfaces, pathcache, pathsearch, scripts, trace_categories, wellknownvars,
};
use crate::{
    builtins, commands, completion, env, error, expansion, functions, jobs, keywords, openfiles,
    prompt, sys::users, traps,
};

/// Type for storing a key bindings helper.
pub type KeyBindingsHelper = Arc<Mutex<dyn interfaces::KeyBindings>>;

/// Type for storing an error formatter.
pub type ErrorFormatterHelper = Arc<Mutex<dyn error::ErrorFormatter>>;

/// Type alias for shell file descriptors.
pub type ShellFd = i32;

/// Represents an instance of a shell.
pub struct Shell {
    /// Trap handler configuration for the shell.
    pub traps: traps::TrapHandlerConfig,

    /// Manages files opened and accessible via redirection operators.
    open_files: openfiles::OpenFiles,

    /// The current working directory.
    working_dir: PathBuf,

    /// The shell environment, containing shell variables.
    pub env: ShellEnvironment,

    /// Shell function definitions.
    funcs: functions::FunctionEnv,

    /// Runtime shell options.
    pub options: RuntimeOptions,

    /// State of managed jobs.
    pub jobs: jobs::JobManager,

    /// Shell aliases.
    pub aliases: HashMap<String, String>,

    /// The status of the last completed command.
    last_exit_status: u8,

    /// The status of each of the commands in the last pipeline.
    pub last_pipeline_statuses: Vec<u8>,

    /// Clone depth from the original ancestor shell.
    depth: usize,

    /// Shell name (a.k.a. $0)
    pub shell_name: Option<String>,

    /// Shell version
    version: Option<String>,

    /// Positional parameters stack ($1 and beyond)
    pub positional_parameters: Vec<String>,

    /// Detailed display string for the shell
    product_display_str: Option<String>,

    /// Script call stack.
    script_call_stack: scripts::CallStack,

    /// Function call stack.
    function_call_stack: functions::CallStack,

    /// Directory stack used by pushd et al.
    pub directory_stack: Vec<PathBuf>,

    /// Current line number being processed.
    current_line_number: u32,

    /// Completion configuration.
    pub completion_config: completion::Config,

    /// Shell built-in commands.
    builtins: HashMap<String, builtins::Registration>,

    /// Shell program location cache.
    pub program_location_cache: pathcache::PathCache,

    /// Last "SECONDS" captured time.
    last_stopwatch_time: std::time::SystemTime,

    /// Last "SECONDS" offset requested.
    last_stopwatch_offset: u32,

    /// Key bindings for the shell, optionally implemented by an interactive shell.
    key_bindings: Option<KeyBindingsHelper>,

    /// History of commands executed in the shell.
    history: Option<history::History>,

    /// Error formatter for customizing error display.
    error_formatter: ErrorFormatterHelper,
}

impl Clone for Shell {
    fn clone(&self) -> Self {
        Self {
            traps: self.traps.clone(),
            open_files: self.open_files.clone(),
            working_dir: self.working_dir.clone(),
            env: self.env.clone(),
            funcs: self.funcs.clone(),
            options: self.options.clone(),
            jobs: jobs::JobManager::new(),
            aliases: self.aliases.clone(),
            last_exit_status: self.last_exit_status,
            last_pipeline_statuses: self.last_pipeline_statuses.clone(),
            positional_parameters: self.positional_parameters.clone(),
            shell_name: self.shell_name.clone(),
            version: self.version.clone(),
            product_display_str: self.product_display_str.clone(),
            function_call_stack: self.function_call_stack.clone(),
            script_call_stack: self.script_call_stack.clone(),
            directory_stack: self.directory_stack.clone(),
            current_line_number: self.current_line_number,
            completion_config: self.completion_config.clone(),
            builtins: self.builtins.clone(),
            program_location_cache: self.program_location_cache.clone(),
            last_stopwatch_time: self.last_stopwatch_time,
            last_stopwatch_offset: self.last_stopwatch_offset,
            key_bindings: self.key_bindings.clone(),
            history: self.history.clone(),
            error_formatter: self.error_formatter.clone(),
            depth: self.depth + 1,
        }
    }
}

impl AsRef<Self> for Shell {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsMut<Self> for Shell {
    fn as_mut(&mut self) -> &mut Self {
        self
    }
}

pub use shell_builder::State as ShellBuilderState;

impl<S: shell_builder::IsComplete> ShellBuilder<S> {
    /// Returns a new shell instance created with the options provided
    pub async fn build(self) -> Result<Shell, error::Error> {
        let options = self.build_settings();

        Shell::new(options).await
    }
}

impl<S: shell_builder::State> ShellBuilder<S> {
    /// Add a disabled option
    pub fn disable_option(mut self, option: impl Into<String>) -> Self {
        self.disabled_options.push(option.into());
        self
    }

    /// Add an enabled option
    pub fn enable_option(mut self, option: impl Into<String>) -> Self {
        self.enabled_options.push(option.into());
        self
    }

    /// Add many disabled options
    pub fn disable_options(mut self, options: impl IntoIterator<Item: Into<String>>) -> Self {
        self.disabled_options
            .extend(options.into_iter().map(Into::into));
        self
    }

    /// Add many enabled options
    pub fn enable_options(mut self, options: impl IntoIterator<Item: Into<String>>) -> Self {
        self.enabled_options
            .extend(options.into_iter().map(Into::into));
        self
    }

    /// Add a disabled shopt option
    pub fn disable_shopt_option(mut self, option: impl Into<String>) -> Self {
        self.disabled_shopt_options.push(option.into());
        self
    }

    /// Add an enabled shopt option
    pub fn enable_shopt_option(mut self, option: impl Into<String>) -> Self {
        self.enabled_shopt_options.push(option.into());
        self
    }

    /// Add many disabled shopt options
    pub fn disable_shopt_options(mut self, options: impl IntoIterator<Item: Into<String>>) -> Self {
        self.disabled_shopt_options
            .extend(options.into_iter().map(Into::into));
        self
    }

    /// Add many enabled shopt options
    pub fn enable_shopt_options(mut self, options: impl IntoIterator<Item: Into<String>>) -> Self {
        self.enabled_shopt_options
            .extend(options.into_iter().map(Into::into));
        self
    }

    /// Add a single builtin registration
    pub fn builtin(mut self, name: impl Into<String>, reg: builtins::Registration) -> Self {
        self.builtins.insert(name.into(), reg);
        self
    }

    /// Add many builtin registrations
    pub fn builtins(
        mut self,
        builtins: impl IntoIterator<Item = (String, builtins::Registration)>,
    ) -> Self {
        self.builtins.extend(builtins);
        self
    }
}

/// Options for creating a new shell.
#[derive(Default, bon::Builder)]
#[builder(
    builder_type(
        name = ShellBuilder,
        doc {
        /// Builder for [Shell]
    }),
    finish_fn(
        name = build_settings,
        vis = "pub(self)",
    ),
    start_fn(
        vis = "pub(self)"
    )
)]
pub struct CreateOptions {
    /// Disabled options.
    #[builder(field)]
    pub disabled_options: Vec<String>,
    /// Enabled options.
    #[builder(field)]
    pub enabled_options: Vec<String>,
    /// Disabled shopt options.
    #[builder(field)]
    pub disabled_shopt_options: Vec<String>,
    /// Enabled shopt options.
    #[builder(field)]
    pub enabled_shopt_options: Vec<String>,
    /// Registered builtins.
    #[builder(field)]
    pub builtins: HashMap<String, builtins::Registration>,
    /// Disallow overwriting regular files via output redirection.
    #[builder(default)]
    pub disallow_overwriting_regular_files_via_output_redirection: bool,
    /// Do not execute commands.
    #[builder(default)]
    pub do_not_execute_commands: bool,
    /// Exit after one command.
    #[builder(default)]
    pub exit_after_one_command: bool,
    /// Whether the shell is interactive.
    #[builder(default)]
    pub interactive: bool,
    /// Whether the shell is a login shell.
    #[builder(default)]
    pub login: bool,
    /// Whether to skip using a readline-like interface for input.
    #[builder(default)]
    pub no_editing: bool,
    /// Whether to skip sourcing the system profile.
    #[builder(default)]
    pub no_profile: bool,
    /// Whether to skip sourcing the user's rc file.
    #[builder(default)]
    pub no_rc: bool,
    /// Explicit override of rc file to load in interactive mode.
    pub rc_file: Option<PathBuf>,
    /// Whether to skip inheriting environment variables from the calling process.
    #[builder(default)]
    pub do_not_inherit_env: bool,
    /// Provides a set of initial open files to be tracked by the shell.
    pub fds: Option<HashMap<ShellFd, openfiles::OpenFile>>,
    /// Whether the shell is in POSIX compliance mode.
    #[builder(default)]
    pub posix: bool,
    /// Whether to print commands and arguments as they are read.
    #[builder(default)]
    pub print_commands_and_arguments: bool,
    /// Whether commands are being read from stdin.
    #[builder(default)]
    pub read_commands_from_stdin: bool,
    /// The name of the shell.
    pub shell_name: Option<String>,
    /// Optionally provides a display string describing the version and variant of the shell.
    pub shell_product_display_str: Option<String>,
    /// Whether to run in maximal POSIX sh compatibility mode.
    #[builder(default)]
    pub sh_mode: bool,
    /// Whether to print verbose output.
    #[builder(default)]
    pub verbose: bool,
    /// Maximum function call depth.
    pub max_function_call_depth: Option<usize>,
    /// Key bindings helper for the shell to use.
    pub key_bindings: Option<KeyBindingsHelper>,
    /// Error formatter helper for the shell to use.
    pub error_formatter: Option<ErrorFormatterHelper>,
    /// Brush implementation version.
    pub shell_version: Option<String>,
}

impl Shell {
    /// Create an instance of [Shell] using the builder syntax
    pub fn builder() -> ShellBuilder<shell_builder::Empty> {
        CreateOptions::builder()
    }

    /// Returns a new shell instance created with the given options.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use when creating the shell.
    pub async fn new(options: CreateOptions) -> Result<Self, error::Error> {
        // Instantiate the shell with some defaults.
        let mut shell = Self {
            traps: traps::TrapHandlerConfig::default(),
            open_files: openfiles::OpenFiles::new(),
            // Populate working directory from the host environment.
            working_dir: std::env::current_dir()?,
            env: env::ShellEnvironment::new(),
            funcs: functions::FunctionEnv::default(),
            options: RuntimeOptions::defaults_from(&options),
            jobs: jobs::JobManager::new(),
            aliases: HashMap::default(),
            last_exit_status: 0,
            last_pipeline_statuses: vec![0],
            positional_parameters: vec![],
            shell_name: options.shell_name,
            version: options.shell_version,
            product_display_str: options.shell_product_display_str,
            function_call_stack: functions::CallStack::new(),
            script_call_stack: scripts::CallStack::new(),
            directory_stack: vec![],
            current_line_number: 0,
            completion_config: completion::Config::default(),
            builtins: options.builtins,
            program_location_cache: pathcache::PathCache::default(),
            last_stopwatch_time: std::time::SystemTime::now(),
            last_stopwatch_offset: 0,
            key_bindings: options.key_bindings,
            history: None,
            error_formatter: options
                .error_formatter
                .unwrap_or_else(|| Arc::new(Mutex::new(error::DefaultErrorFormatter::new()))),
            depth: 0,
        };

        // Add in any open files provided.
        if let Some(fds) = options.fds {
            shell.open_files.update_from(fds.into_iter());
        }

        // TODO: Without this a script that sets extglob will fail because we
        // parse the entire script with the same settings.
        shell.options.extended_globbing = true;

        // Initialize environment.
        wellknownvars::initialize_vars(&mut shell, options.do_not_inherit_env)?;

        // Set up history, if relevant.
        if shell.options.enable_command_history {
            if let Some(history_path) = shell.history_file_path() {
                let mut options = std::fs::File::options();
                options.read(true);

                if let Ok(history_file) =
                    shell.open_file(&options, history_path, &shell.default_exec_params())
                {
                    shell.history = Some(history::History::import(history_file)?);
                }
            }

            if shell.history.is_none() {
                shell.history = Some(history::History::default());
            }
        }

        // Load profiles/configuration.
        shell
            .load_config(
                options.no_profile,
                options.no_rc,
                options.rc_file.as_deref(),
            )
            .await?;

        Ok(shell)
    }

    /// Returns the current source line number being processed.
    pub const fn current_line_number(&self) -> u32 {
        self.current_line_number
    }

    /// Returns the shell's official version string (if available).
    pub const fn version(&self) -> &Option<String> {
        &self.version
    }

    /// Returns the exit status of the last command executed in this shell.
    pub const fn last_result(&self) -> u8 {
        self.last_exit_status
    }

    /// Returns a reference to the current function call stack for the shell.
    pub const fn function_call_stack(&self) -> &functions::CallStack {
        &self.function_call_stack
    }

    /// Returns a reference to the current script call stack for the shell.
    pub const fn script_call_stack(&self) -> &scripts::CallStack {
        &self.script_call_stack
    }

    /// Returns a mutable reference to the last exit status.
    pub const fn last_exit_status_mut(&mut self) -> &mut u8 {
        &mut self.last_exit_status
    }

    /// Returns the key bindings helper for the shell.
    pub const fn key_bindings(&self) -> &Option<KeyBindingsHelper> {
        &self.key_bindings
    }

    /// Returns the registered builtins for the shell.
    pub const fn builtins(&self) -> &HashMap<String, builtins::Registration> {
        &self.builtins
    }

    /// Returns the shell's current working directory.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// Returns a mutable reference to the shell's current working directory.
    /// This is only accessible within the crate.
    pub(crate) const fn working_dir_mut(&mut self) -> &mut PathBuf {
        &mut self.working_dir
    }

    /// Returns the product display name for this shell.
    pub const fn product_display_str(&self) -> &Option<String> {
        &self.product_display_str
    }

    /// Returns the function definition environment for this shell.
    pub const fn funcs(&self) -> &functions::FunctionEnv {
        &self.funcs
    }

    /// Tries to undefine a function in the shell's environment. Returns whether or
    /// not a definition was removed.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to undefine.
    pub fn undefine_func(&mut self, name: &str) -> bool {
        self.funcs.remove(name).is_some()
    }

    /// Defines a function in the shell's environment. If a function already exists
    /// with the given name, it is replaced with the new definition.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to define.
    /// * `definition` - The function's definition.
    pub fn define_func(
        &mut self,
        name: impl Into<String>,
        definition: brush_parser::ast::FunctionDefinition,
    ) {
        self.funcs.update(name.into(), definition.into());
    }

    /// Tries to return a mutable reference to the registration for a named function.
    /// Returns `None` if no such function was found.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to lookup
    pub fn func_mut(&mut self, name: &str) -> Option<&mut functions::Registration> {
        self.funcs.get_mut(name)
    }

    /// Tries to define a function in the shell's environment using the given
    /// string as its body.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function
    /// * `body_text` - The body of the function, expected to start with "()".
    pub fn define_func_from_str(
        &mut self,
        name: impl Into<String>,
        body_text: &str,
    ) -> Result<(), error::Error> {
        let name = name.into();

        let mut parser = create_parser(body_text.as_bytes(), &self.parser_options());
        let func_body = parser.parse_function_parens_and_body().map_err(|e| {
            error::Error::from(error::ErrorKind::FunctionParseError(name.clone(), e))
        })?;

        let def = brush_parser::ast::FunctionDefinition {
            fname: name.clone().into(),
            body: func_body,
            source: String::new(),
        };

        self.define_func(name, def);

        Ok(())
    }

    /// Returns the last "SECONDS" captured time.
    pub const fn last_stopwatch_time(&self) -> std::time::SystemTime {
        self.last_stopwatch_time
    }

    /// Returns the last "SECONDS" offset requested.
    pub const fn last_stopwatch_offset(&self) -> u32 {
        self.last_stopwatch_offset
    }

    async fn load_config(
        &mut self,
        skip_profile: bool,
        skip_rc: bool,
        rc_file: Option<&Path>,
    ) -> Result<(), error::Error> {
        let mut params = self.default_exec_params();
        params.process_group_policy = interp::ProcessGroupPolicy::SameProcessGroup;

        if self.options.login_shell {
            // --noprofile means skip this.
            if skip_profile {
                return Ok(());
            }

            //
            // Source /etc/profile if it exists.
            //
            // Next source the first of these that exists and is readable (if any):
            //     * ~/.bash_profile
            //     * ~/.bash_login
            //     * ~/.profile
            //
            self.source_if_exists(Path::new("/etc/profile"), &params)
                .await?;
            if let Some(home_path) = self.home_dir() {
                if self.options.sh_mode {
                    self.source_if_exists(home_path.join(".profile").as_path(), &params)
                        .await?;
                } else {
                    if !self
                        .source_if_exists(home_path.join(".bash_profile").as_path(), &params)
                        .await?
                    {
                        if !self
                            .source_if_exists(home_path.join(".bash_login").as_path(), &params)
                            .await?
                        {
                            self.source_if_exists(home_path.join(".profile").as_path(), &params)
                                .await?;
                        }
                    }
                }
            }
        } else {
            if self.options.interactive {
                // --norc means skip this. Also skip in sh mode.
                if skip_rc || self.options.sh_mode {
                    return Ok(());
                }

                // If an rc file was specified, then source it.
                if let Some(rc_file) = rc_file {
                    // If an explicit rc file is provided, source it.
                    self.source_if_exists(rc_file, &params).await?;
                } else {
                    //
                    // Otherwise, for non-login interactive shells, load in this order:
                    //
                    //     /etc/bash.bashrc
                    //     ~/.bashrc
                    //
                    self.source_if_exists(Path::new("/etc/bash.bashrc"), &params)
                        .await?;
                    if let Some(home_path) = self.home_dir() {
                        self.source_if_exists(home_path.join(".bashrc").as_path(), &params)
                            .await?;
                        self.source_if_exists(home_path.join(".brushrc").as_path(), &params)
                            .await?;
                    }
                }
            } else {
                let env_var_name = if self.options.sh_mode {
                    "ENV"
                } else {
                    "BASH_ENV"
                };

                if self.env.is_set(env_var_name) {
                    //
                    // TODO: look at $ENV/BASH_ENV; source its expansion if that file exists
                    //
                    return error::unimp(
                        "load config from $ENV/BASH_ENV for non-interactive, non-login shell",
                    );
                }
            }
        }

        Ok(())
    }

    async fn source_if_exists(
        &mut self,
        path: impl AsRef<Path>,
        params: &ExecutionParameters,
    ) -> Result<bool, error::Error> {
        let path = path.as_ref();
        if path.exists() {
            self.source_script(path, std::iter::empty::<String>(), params)
                .await?;
            Ok(true)
        } else {
            tracing::debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    pub async fn source_script<S: AsRef<str>, P: AsRef<Path>, I: Iterator<Item = S>>(
        &mut self,
        path: P,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        self.parse_and_execute_script_file(path.as_ref(), args, params, scripts::CallType::Sourced)
            .await
    }

    /// Parse and execute the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn parse_and_execute_script_file<S: AsRef<str>, P: AsRef<Path>, I: Iterator<Item = S>>(
        &mut self,
        path: P,
        args: I,
        params: &ExecutionParameters,
        call_type: scripts::CallType,
    ) -> Result<ExecutionResult, error::Error> {
        let path = path.as_ref();
        tracing::debug!("sourcing: {}", path.display());

        let mut options = std::fs::File::options();
        options.read(true);

        let opened_file: openfiles::OpenFile = self
            .open_file(&options, path, params)
            .map_err(|e| error::ErrorKind::FailedSourcingFile(path.to_owned(), e))?;

        if opened_file.is_dir() {
            return Err(error::ErrorKind::FailedSourcingFile(
                path.to_owned(),
                std::io::Error::from(std::io::ErrorKind::IsADirectory),
            )
            .into());
        }

        let source_info = brush_parser::SourceInfo {
            source: path.to_string_lossy().to_string(),
        };

        let mut result = self
            .source_file(opened_file, &source_info, args, params, call_type)
            .await?;

        // Handle control flow at script execution boundary. If execution completed
        // with a `return`, we need to clear it since it's already been "used". All
        // other control flow types are preserved.
        if matches!(
            result.next_control_flow,
            ExecutionControlFlow::ReturnFromFunctionOrScript
        ) {
            result.next_control_flow = ExecutionControlFlow::Normal;
        }

        Ok(result)
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `file` - The file to source.
    /// * `source_info` - Information about the source of the script.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn source_file<F: Read, S: AsRef<str>, I: Iterator<Item = S>>(
        &mut self,
        file: F,
        source_info: &brush_parser::SourceInfo,
        args: I,
        params: &ExecutionParameters,
        call_type: scripts::CallType,
    ) -> Result<ExecutionResult, error::Error> {
        let mut reader = std::io::BufReader::new(file);
        let mut parser =
            brush_parser::Parser::new(&mut reader, &self.parser_options(), source_info);

        tracing::debug!(target: trace_categories::PARSE, "Parsing sourced file: {}", source_info.source);
        let parse_result = parser.parse_program();

        let mut other_positional_parameters: Vec<_> = args.map(|s| s.as_ref().to_owned()).collect();
        let mut other_shell_name = Some(source_info.source.clone());
        let positional_params_given = !other_positional_parameters.is_empty();

        // TODO: Find a cleaner way to change args.
        std::mem::swap(&mut self.shell_name, &mut other_shell_name);

        // NOTE: We only shadow the original positional parameters if any were explicitly given
        // for the script sourcing.
        if positional_params_given {
            std::mem::swap(
                &mut self.positional_parameters,
                &mut other_positional_parameters,
            );
        }

        self.script_call_stack
            .push(call_type, source_info.source.clone());

        let result = self
            .run_parsed_result(parse_result, source_info, params)
            .await;

        self.script_call_stack.pop();

        // Restore.
        std::mem::swap(&mut self.shell_name, &mut other_shell_name);

        // We only restore the original positional parameters if we needed to shadow them.
        if positional_params_given {
            std::mem::swap(
                &mut self.positional_parameters,
                &mut other_positional_parameters,
            );
        }

        result
    }

    /// Invokes a function defined in this shell, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to invoke.
    /// * `args` - The arguments to pass to the function.
    /// * `params` - Execution parameters to use for the invocation.
    pub async fn invoke_function<N: AsRef<str>, I: IntoIterator<Item = A>, A: AsRef<str>>(
        &mut self,
        name: N,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<u8, error::Error> {
        let name = name.as_ref();
        let command_name = String::from(name);

        let func_registration = self
            .funcs
            .get(name)
            .ok_or_else(|| error::ErrorKind::FunctionNotFound(name.to_owned()))?;

        let func = func_registration.definition.clone();

        let context = commands::ExecutionContext {
            shell: self,
            command_name,
            params: params.clone(),
        };

        let command_args = args
            .into_iter()
            .map(|s| commands::CommandArg::String(String::from(s.as_ref())))
            .collect::<Vec<_>>();

        match commands::invoke_shell_function(func, context, &command_args).await? {
            ExecutionSpawnResult::StartedProcess(_) => {
                error::unimp("child spawned from function invocation")
            }
            ExecutionSpawnResult::Completed(result) => Ok(result.exit_code.into()),
        }
    }

    /// Executes the given string as a shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute.
    /// * `params` - Execution parameters.
    pub async fn run_string<S: Into<String>>(
        &mut self,
        command: S,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // TODO: Actually track line numbers; this is something of a hack, assuming each time
        // this function is invoked we are on the next line of the input. For one thing,
        // each string we run could be multiple lines.
        self.current_line_number += 1;

        let parse_result = self.parse_string(command.into());
        let source_info = brush_parser::SourceInfo {
            source: String::from("main"),
        };
        self.run_parsed_result(parse_result, &source_info, params)
            .await
    }

    /// Parses the given reader as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    pub fn parse<R: Read>(
        &self,
        reader: R,
    ) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
        let mut parser = create_parser(reader, &self.parser_options());

        tracing::debug!(target: trace_categories::PARSE, "Parsing reader as program...");
        parser.parse_program()
    }

    /// Parses the given string as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to parse as a program.
    pub fn parse_string<S: Into<String>>(
        &self,
        s: S,
    ) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
        parse_string_impl(s.into(), self.parser_options())
    }

    /// Applies basic shell expansion to the provided string.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand.
    pub async fn basic_expand_string<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<String, error::Error> {
        let result = expansion::basic_expand_str(self, params, s.as_ref()).await?;
        Ok(result)
    }

    /// Applies full shell expansion and field splitting to the provided string; returns
    /// a sequence of fields.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand and split.
    pub async fn full_expand_and_split_string<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<Vec<String>, error::Error> {
        let result = expansion::full_expand_and_split_str(self, params, s.as_ref()).await?;
        Ok(result)
    }

    /// Returns the default execution parameters for this shell.
    pub fn default_exec_params(&self) -> ExecutionParameters {
        ExecutionParameters::default()
    }

    /// Executes the given script file, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `script_path` - The path to the script file to execute.
    /// * `args` - The arguments to pass to the script as positional parameters.
    pub async fn run_script<S: AsRef<str>, P: AsRef<Path>, I: Iterator<Item = S>>(
        &mut self,
        script_path: P,
        args: I,
    ) -> Result<ExecutionResult, error::Error> {
        let params = self.default_exec_params();
        let result = self
            .parse_and_execute_script_file(
                script_path.as_ref(),
                args,
                &params,
                scripts::CallType::Executed,
            )
            .await?;

        let _ = self.on_exit().await;

        Ok(result)
    }

    /// Runs any exit steps for the shell.
    pub async fn on_exit(&mut self) -> Result<(), error::Error> {
        self.invoke_exit_trap_handler_if_registered().await?;

        Ok(())
    }

    async fn invoke_exit_trap_handler_if_registered(
        &mut self,
    ) -> Result<ExecutionResult, error::Error> {
        let Some(handler) = self.traps.handlers.get(&traps::TrapSignal::Exit).cloned() else {
            return Ok(ExecutionResult::success());
        };

        // TODO: Confirm whether trap handlers should be executed in the same process group.
        let mut params = self.default_exec_params();
        params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

        let orig_last_exit_status = self.last_exit_status;
        self.traps.handler_depth += 1;

        let result = self.run_string(handler, &params).await;

        self.traps.handler_depth -= 1;
        self.last_exit_status = orig_last_exit_status;

        result
    }

    pub(crate) async fn run_parsed_result(
        &mut self,
        parse_result: Result<brush_parser::ast::Program, brush_parser::ParseError>,
        source_info: &brush_parser::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // If parsing succeeded, run the program.
        let result = match parse_result {
            Ok(prog) => self.run_program(prog, params).await,
            Err(parse_err) => Err(error::Error::from(error::ErrorKind::ParseError(
                parse_err,
                source_info.clone(),
            ))),
        };

        // Report any errors.
        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                let _ = self.display_error(&mut params.stderr(self), &err).await;
                let exit_code = ExecutionExitCode::from(&err);
                *self.last_exit_status_mut() = exit_code.into();
                Ok(exit_code.into())
            }
        }
    }

    /// Executes the given parsed shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `program` - The program to execute.
    /// * `params` - Execution parameters.
    pub async fn run_program(
        &mut self,
        program: brush_parser::ast::Program,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        program.execute(self, params).await
    }

    const fn default_prompt(&self) -> &'static str {
        if self.options.sh_mode {
            "$ "
        } else {
            "brush$ "
        }
    }

    /// Composes the shell's post-input, pre-command prompt, applying all appropriate expansions.
    pub async fn compose_precmd_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS0", "").await
    }

    /// Composes the shell's prompt, applying all appropriate expansions.
    pub async fn compose_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS1", self.default_prompt()).await
    }

    /// Compose's the shell's alternate-side prompt, applying all appropriate expansions.
    pub async fn compose_alt_side_prompt(&mut self) -> Result<String, error::Error> {
        // This is a brush extension.
        self.expand_prompt_var("BRUSH_PS_ALT", "").await
    }

    /// Composes the shell's continuation prompt.
    pub async fn compose_continuation_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS2", "> ").await
    }

    async fn expand_prompt_var(
        &mut self,
        var_name: &str,
        default: &str,
    ) -> Result<String, error::Error> {
        //
        // TODO(prompt): bash appears to do this in a subshell; we need to investigate
        // if that's required.
        //

        // Retrieve the spec.
        let prompt_spec = self.parameter_or_default(var_name, default);
        if prompt_spec.is_empty() {
            return Ok(String::new());
        }

        // Expand it.
        let params = self.default_exec_params();
        prompt::expand_prompt(self, &params, prompt_spec.into_owned()).await
    }

    fn parameter_or_default<'a>(&'a self, name: &str, default: &'a str) -> Cow<'a, str> {
        self.env_str(name).unwrap_or_else(|| default.into())
    }

    /// Returns the options that should be used for parsing shell programs; reflects
    /// the current configuration state of the shell and may change over time.
    pub const fn parser_options(&self) -> brush_parser::ParserOptions {
        brush_parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
            sh_mode: self.options.sh_mode,
            tilde_expansion: true,
        }
    }

    /// Returns whether or not the shell is actively executing in a sourced script.
    pub fn in_sourced_script(&self) -> bool {
        self.script_call_stack.in_sourced_script()
    }

    /// Returns whether or not the shell is actively executing in a shell function.
    pub fn in_function(&self) -> bool {
        !self.function_call_stack.is_empty()
    }

    /// Updates the shell's internal tracking state to reflect that a new shell
    /// function is being entered.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function being entered.
    /// * `function_def` - The definition of the function being entered.
    pub(crate) fn enter_function(
        &mut self,
        name: &str,
        function_def: &Arc<brush_parser::ast::FunctionDefinition>,
    ) -> Result<(), error::Error> {
        if let Some(max_call_depth) = self.options.max_function_call_depth {
            if self.function_call_stack.depth() >= max_call_depth {
                return Err(error::ErrorKind::MaxFunctionCallDepthExceeded.into());
            }
        }

        if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
            let depth = self.function_call_stack.depth();
            let prefix = repeated_char_str(' ', depth);
            tracing::debug!(target: trace_categories::FUNCTIONS, "Entering func [depth={depth}]: {prefix}{name}");
        }

        self.function_call_stack.push(name, function_def);
        self.env.push_scope(env::EnvironmentScope::Local);

        Ok(())
    }

    /// Updates the shell's internal tracking state to reflect that the shell
    /// has exited the top-most function on its call stack.
    pub(crate) fn leave_function(&mut self) -> Result<(), error::Error> {
        self.env.pop_scope(env::EnvironmentScope::Local)?;

        if let Some(exited_call) = self.function_call_stack.pop() {
            if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
                let depth = self.function_call_stack.depth();
                let prefix = repeated_char_str(' ', depth);
                tracing::debug!(target: trace_categories::FUNCTIONS, "Exiting func  [depth={depth}]: {prefix}{}", exited_call.function_name);
            }
        }

        Ok(())
    }

    /// Returns the path to the history file used by the shell, if one is set.
    pub fn history_file_path(&self) -> Option<PathBuf> {
        self.env_str("HISTFILE")
            .map(|s| PathBuf::from(s.into_owned()))
    }

    /// Returns the path to the history file used by the shell, if one is set.
    pub fn history_time_format(&self) -> Option<String> {
        self.env_str("HISTTIMEFORMAT").map(|s| s.into_owned())
    }

    /// Saves history back to any backing storage.
    pub fn save_history(&mut self) -> Result<(), error::Error> {
        if let Some(history_file_path) = self.history_file_path() {
            if let Some(history) = &mut self.history {
                // See if there's *any* time format configured. That triggers writing out timestamps.
                let write_timestamps = self.env.is_set("HISTTIMEFORMAT");

                // TODO: Observe options.append_to_history_file
                history.flush(
                    history_file_path,
                    true, /*append?*/
                    true, /*unsaved items only?*/
                    write_timestamps,
                )?;
            }
        }

        Ok(())
    }

    /// Adds a command to history.
    pub fn add_to_history(&mut self, command: &str) -> Result<(), error::Error> {
        if let Some(history) = &mut self.history {
            // Trim.
            let command = command.trim();

            // For now, discard empty commands.
            if command.is_empty() {
                return Ok(());
            }

            // Add it to history.
            history.add(history::Item {
                id: 0,
                command_line: command.to_owned(),
                timestamp: Some(chrono::Utc::now()),
                dirty: true,
            })?;
        }

        Ok(())
    }

    /// Tries to retrieve a variable from the shell's environment, converting it into its
    /// string form.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn env_str(&self, name: &str) -> Option<Cow<'_, str>> {
        self.env.get_str(name, self)
    }

    /// Tries to retrieve a variable from the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn env_var(&self, name: &str) -> Option<&ShellVariable> {
        self.env.get(name).map(|(_, var)| var)
    }

    /// Tries to set a global variable in the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to add.
    /// * `var` - The variable contents to add.
    pub fn set_env_global(&mut self, name: &str, var: ShellVariable) -> Result<(), error::Error> {
        self.env.set_global(name, var)
    }

    /// Register a builtin to the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The in-shell name of the builtin.
    /// * `registration` - The registration handle for the builtin.
    pub fn register_builtin<S: Into<String>>(
        &mut self,
        name: S,
        registration: builtins::Registration,
    ) {
        self.builtins.insert(name.into(), registration);
    }

    /// Tries to retrieve a mutable reference to an existing builtin registration.
    /// Returns `None` if no such registration exists.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the builtin to lookup.
    pub fn builtin_mut(&mut self, name: &str) -> Option<&mut builtins::Registration> {
        self.builtins.get_mut(name)
    }

    /// Returns the current value of the IFS variable, or the default value if it is not set.
    pub fn ifs(&self) -> Cow<'_, str> {
        self.env_str("IFS").unwrap_or_else(|| " \t\n".into())
    }

    /// Returns the first character of the IFS variable, or a space if it is not set.
    pub(crate) fn get_ifs_first_char(&self) -> char {
        self.ifs().chars().next().unwrap_or(' ')
    }

    /// Generates command completions for the shell.
    ///
    /// # Arguments
    ///
    /// * `input` - The input string to generate completions for.
    /// * `position` - The position in the input string to generate completions at.
    pub async fn complete(
        &mut self,
        input: &str,
        position: usize,
    ) -> Result<completion::Completions, error::Error> {
        let completion_config = self.completion_config.clone();
        completion_config
            .get_completions(self, input, position)
            .await
    }

    /// Finds executables in the shell's current default PATH, matching the given glob pattern.
    ///
    /// # Arguments
    ///
    /// * `required_glob_pattern` - The glob pattern to match against.
    pub fn find_executables_in_path<'a>(
        &'a self,
        filename: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let paths = path_var.split(':').map(|s| s.to_owned());

        pathsearch::search_for_executable(paths.into_iter(), filename)
    }

    /// Finds executables in the shell's current default PATH, with filenames matching the
    /// given prefix.
    ///
    /// # Arguments
    ///
    /// * `filename_prefix` - The prefix to match against executable filenames.
    pub fn find_executables_in_path_with_prefix(
        &self,
        filename_prefix: &str,
        case_insensitive: bool,
    ) -> impl Iterator<Item = PathBuf> {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let paths = path_var.split(':').map(|s| s.to_owned());

        pathsearch::search_for_executable_with_prefix(
            paths.into_iter(),
            filename_prefix,
            case_insensitive,
        )
    }

    /// Determines whether the given filename is the name of an executable in one of the
    /// directories in the shell's current PATH. If found, returns the path.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path<S: AsRef<str>>(
        &self,
        candidate_name: S,
    ) -> Option<PathBuf> {
        for dir_str in self.env_str("PATH").unwrap_or_default().split(':') {
            let candidate_path = Path::new(dir_str).join(candidate_name.as_ref());
            if candidate_path.executable() {
                return Some(candidate_path);
            }
        }
        None
    }

    /// Uses the shell's hash-based path cache to check whether the given filename is the name
    /// of an executable in one of the directories in the shell's current PATH. If found,
    /// ensures the path is in the cache and returns it.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path_using_cache<S: AsRef<str>>(
        &mut self,
        candidate_name: S,
    ) -> Option<PathBuf> {
        if let Some(cached_path) = self.program_location_cache.get(&candidate_name) {
            Some(cached_path)
        } else if let Some(found_path) = self.find_first_executable_in_path(&candidate_name) {
            self.program_location_cache
                .set(&candidate_name, found_path.clone());
            Some(found_path)
        } else {
            None
        }
    }

    /// Gets the absolute form of the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to get the absolute form of.
    pub fn absolute_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.as_os_str().is_empty() || path.is_absolute() {
            path.to_owned()
        } else {
            self.working_dir().join(path)
        }
    }

    /// Opens the given file, using the context of this shell and the provided execution parameters.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use opening the file.
    /// * `path` - The path to the file to open; may be relative to the shell's working directory.
    /// * `params` - Execution parameters.
    pub(crate) fn open_file(
        &self,
        options: &std::fs::OpenOptions,
        path: impl AsRef<Path>,
        params: &ExecutionParameters,
    ) -> Result<openfiles::OpenFile, std::io::Error> {
        let path_to_open = self.absolute_path(path.as_ref());

        // See if this is a reference to a file descriptor, in which case the actual
        // /dev/fd* file path for this process may not match with what's in the execution
        // parameters.
        if let Some(parent) = path_to_open.parent() {
            if parent == Path::new("/dev/fd") {
                if let Some(filename) = path_to_open.file_name() {
                    if let Ok(fd_num) = filename.to_string_lossy().to_string().parse::<ShellFd>() {
                        if let Some(open_file) = params.try_fd(self, fd_num) {
                            return open_file.try_clone();
                        }
                    }
                }
            }
        }

        Ok(options.open(path_to_open)?.into())
    }

    /// Sets the shell's current working directory to the given path.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The path to set as the working directory.
    pub fn set_working_dir(&mut self, target_dir: impl AsRef<Path>) -> Result<(), error::Error> {
        let abs_path = self.absolute_path(target_dir.as_ref());

        match std::fs::metadata(&abs_path) {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::ErrorKind::NotADirectory(abs_path).into());
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // Normalize the path (but don't canonicalize it).
        let cleaned_path = abs_path.normalize();

        let pwd = cleaned_path.to_string_lossy().to_string();

        self.env.update_or_add(
            "PWD",
            variables::ShellValueLiteral::Scalar(pwd),
            |var| {
                var.export();
                Ok(())
            },
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;
        let oldpwd = std::mem::replace(self.working_dir_mut(), cleaned_path);

        self.env.update_or_add(
            "OLDPWD",
            variables::ShellValueLiteral::Scalar(oldpwd.to_string_lossy().to_string()),
            |var| {
                var.export();
                Ok(())
            },
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    /// Tilde-shortens the given string, replacing the user's home directory with a tilde.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to shorten.
    pub fn tilde_shorten(&self, s: String) -> String {
        if let Some(home_dir) = self.home_dir() {
            if let Some(stripped) = s.strip_prefix(home_dir.to_string_lossy().as_ref()) {
                return format!("~{stripped}");
            }
        }
        s
    }

    /// Returns the shell's current home directory, if available.
    pub(crate) fn home_dir(&self) -> Option<PathBuf> {
        if let Some(home) = self.env.get_str("HOME", self) {
            Some(PathBuf::from(home.to_string()))
        } else {
            // HOME isn't set, so let's sort it out ourselves.
            users::get_current_user_home_dir()
        }
    }

    /// Replaces the shell's currently configured open files with the given set.
    /// Typically only used by exec-like builtins.
    ///
    /// # Arguments
    ///
    /// * `open_files` - The new set of open files to use.
    pub fn replace_open_files(
        &mut self,
        open_fds: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) {
        self.open_files = openfiles::OpenFiles::from(open_fds);
    }

    pub(crate) const fn persistent_open_files(&self) -> &openfiles::OpenFiles {
        &self.open_files
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard output stream using `write!` at al.
    pub fn stdout(&self) -> impl std::io::Write {
        self.open_files.try_stdout().cloned().unwrap()
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard error stream using `write!` et al.
    pub fn stderr(&self) -> impl std::io::Write {
        self.open_files.try_stderr().cloned().unwrap()
    }

    /// Outputs `set -x` style trace output for a command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to trace.
    pub(crate) async fn trace_command<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        command: S,
    ) -> Result<(), error::Error> {
        // Expand the PS4 prompt variable to get our prefix.
        let ps4 = self.as_mut().expand_prompt_var("PS4", "").await?;
        let mut prefix = ps4;

        // Add additional depth-based prefixes using the first character of PS4.
        let additional_depth = self.script_call_stack.depth() + self.depth;
        if let Some(c) = prefix.chars().next() {
            for _ in 0..additional_depth {
                prefix.insert(0, c);
            }
        }

        // Resolve which file descriptor to use for tracing. We default to stderr.
        let mut trace_file = params.try_stderr(self);

        // If BASH_XTRACEFD is set and refers to a valid file descriptor, use that instead.
        if let Some((_, xtracefd_var)) = self.env.get("BASH_XTRACEFD") {
            let xtracefd_value = xtracefd_var.value().to_cow_str(self);
            if let Ok(fd) = xtracefd_value.parse::<ShellFd>() {
                if let Some(file) = self.open_files.try_fd(fd) {
                    trace_file = Some(file.clone());
                }
            }
        }

        // If we have a valid trace file, write to it.
        if let Some(trace_file) = trace_file {
            let mut trace_file = trace_file.try_clone()?;
            writeln!(trace_file, "{prefix}{}", command.as_ref())?;
        }

        Ok(())
    }

    /// Returns the keywords that are reserved by the shell.
    pub(crate) fn get_keywords(&self) -> Vec<String> {
        if self.options.sh_mode {
            keywords::SH_MODE_KEYWORDS.iter().cloned().collect()
        } else {
            keywords::KEYWORDS.iter().cloned().collect()
        }
    }

    /// Checks if the given string is a keyword reserved in this shell.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to check.
    pub fn is_keyword(&self, s: &str) -> bool {
        if self.options.sh_mode {
            keywords::SH_MODE_KEYWORDS.contains(s)
        } else {
            keywords::KEYWORDS.contains(s)
        }
    }

    /// Checks for completed jobs in the shell, reporting any changes found.
    pub fn check_for_completed_jobs(&mut self) -> Result<(), error::Error> {
        let results = self.jobs.poll()?;

        if self.options.enable_job_control {
            for (job, _result) in results {
                writeln!(self.stderr(), "{job}")?;
            }
        }

        Ok(())
    }

    /// Evaluate the given arithmetic expression, returning the result.
    pub fn eval_arithmetic(
        &mut self,
        expr: &brush_parser::ast::ArithmeticExpr,
    ) -> Result<i64, error::Error> {
        Ok(expr.eval(self)?)
    }

    /// Updates the shell state to reflect the given edit buffer contents.
    ///
    /// # Arguments
    ///
    /// * `contents` - The contents of the edit buffer.
    /// * `cursor` - The cursor position in the edit buffer.
    pub fn set_edit_buffer(&mut self, contents: String, cursor: usize) -> Result<(), error::Error> {
        self.env
            .set_global("READLINE_LINE", ShellVariable::new(contents))?;

        self.env
            .set_global("READLINE_POINT", ShellVariable::new(cursor.to_string()))?;

        Ok(())
    }

    /// Returns the contents of the shell's edit buffer, if any. The buffer
    /// state is cleared from the shell.
    pub fn pop_edit_buffer(&mut self) -> Result<Option<(String, usize)>, error::Error> {
        let line = self
            .env
            .unset("READLINE_LINE")?
            .map(|line| line.value().to_cow_str(self).to_string());

        let point = self
            .env
            .unset("READLINE_POINT")?
            .and_then(|point| point.value().to_cow_str(self).parse::<usize>().ok())
            .unwrap_or(0);

        if let Some(line) = line {
            Ok(Some((line, point)))
        } else {
            Ok(None)
        }
    }

    /// Returns the shell's history, if it exists.
    pub const fn history(&self) -> Option<&history::History> {
        self.history.as_ref()
    }

    /// Returns a mutable reference to the shell's history, if it exists.
    pub const fn history_mut(&mut self) -> Option<&mut history::History> {
        self.history.as_mut()
    }

    /// Returns whether or not this shell is a subshell.
    pub const fn is_subshell(&self) -> bool {
        self.depth > 0
    }

    /// Returns the current subshell depth; 0 is returned if this shell is not a subshell.
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Displays the given error to the user, using the shell's error display mechanisms.
    ///
    /// # Arguments
    ///
    /// * `file_table` - The open file table to use for any file descriptor references.
    /// * `err` - The error to display.
    pub async fn display_error(
        &self,
        file: &mut impl std::io::Write,
        err: &error::Error,
    ) -> Result<(), error::Error> {
        let str = self.error_formatter.lock().await.format_error(err, self);
        write!(file, "{str}")?;

        Ok(())
    }
}

#[cached::proc_macro::cached(size = 64, result = true)]
fn parse_string_impl(
    s: String,
    parser_options: brush_parser::ParserOptions,
) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
    let mut parser = create_parser(s.as_bytes(), &parser_options);

    tracing::debug!(target: trace_categories::PARSE, "Parsing string as program...");
    parser.parse_program()
}

fn create_parser<R: Read>(
    r: R,
    parser_options: &brush_parser::ParserOptions,
) -> brush_parser::Parser<std::io::BufReader<R>> {
    let reader = std::io::BufReader::new(r);
    let source_info = brush_parser::SourceInfo {
        source: String::from("main"),
    };

    brush_parser::Parser::new(reader, parser_options, &source_info)
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}
