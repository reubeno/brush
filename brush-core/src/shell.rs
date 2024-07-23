use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::arithmetic::Evaluatable;
use crate::env::{EnvironmentLookup, EnvironmentScope, ShellEnvironment};
use crate::interp::{Execute, ExecutionParameters, ExecutionResult};
use crate::options::RuntimeOptions;
use crate::sys::fs::PathExt;
use crate::variables::{self, ShellValue, ShellVariable};
use crate::{
    builtins, commands, completion, env, error, expansion, functions, jobs, keywords, openfiles,
    patterns, prompt, sys::users, traps,
};

/// Represents an instance of a shell.
pub struct Shell {
    //
    // Core state required by specification
    /// Trap handler configuration for the shell.
    pub traps: traps::TrapHandlerConfig,
    /// Manages files opened and accessible via redirection operators.
    pub open_files: openfiles::OpenFiles,
    /// The current working directory.
    pub working_dir: PathBuf,
    /// The shell environment, containing shell variables.
    pub env: ShellEnvironment,
    /// Shell function definitions.
    pub funcs: functions::FunctionEnv,
    /// Runtime shell options.
    pub options: RuntimeOptions,
    /// State of managed jobs.
    pub jobs: jobs::JobManager,
    /// Shell aliases.
    pub aliases: HashMap<String, String>,

    //
    // Additional state
    /// The status of the last completed command.
    pub last_exit_status: u8,

    /// Clone depth from the original ancestor shell.
    pub depth: usize,

    /// Positional parameters ($1 and beyond)
    pub positional_parameters: Vec<String>,

    /// Shell name
    pub shell_name: Option<String>,

    /// Detailed display string for the shell
    pub shell_product_display_str: Option<String>,

    /// Script call stack.
    pub script_call_stack: VecDeque<String>,

    /// Function call stack.
    pub function_call_stack: VecDeque<FunctionCall>,

    /// Directory stack used by pushd et al.
    pub directory_stack: Vec<PathBuf>,

    /// Current line number being processed.
    pub current_line_number: u32,

    /// Completion configuration.
    pub completion_config: completion::Config,

    /// Shell built-in commands.
    pub builtins: HashMap<String, builtins::Registration>,
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
            positional_parameters: self.positional_parameters.clone(),
            shell_name: self.shell_name.clone(),
            shell_product_display_str: self.shell_product_display_str.clone(),
            function_call_stack: self.function_call_stack.clone(),
            script_call_stack: self.script_call_stack.clone(),
            directory_stack: self.directory_stack.clone(),
            current_line_number: self.current_line_number,
            completion_config: self.completion_config.clone(),
            builtins: self.builtins.clone(),
            depth: self.depth + 1,
        }
    }
}

/// Options for creating a new shell.
#[derive(Debug, Default)]
pub struct CreateOptions {
    /// Disabled shopt options.
    pub disabled_shopt_options: Vec<String>,
    /// Enabled shopt options.
    pub enabled_shopt_options: Vec<String>,
    /// Do not execute commands.
    pub do_not_execute_commands: bool,
    /// Whether the shell is interactive.
    pub interactive: bool,
    /// Whether the shell is a login shell.
    pub login: bool,
    /// Whether to skip using a readline-like interface for input.
    pub no_editing: bool,
    /// Whether to skip sourcing the system profile.
    pub no_profile: bool,
    /// Whether to skip sourcing the user's rc file.
    pub no_rc: bool,
    /// Whether the shell is in POSIX compliance mode.
    pub posix: bool,
    /// Whether to print commands and arguments as they are read.
    pub print_commands_and_arguments: bool,
    /// Whether commands are being read from stdin.
    pub read_commands_from_stdin: bool,
    /// The name of the shell.
    pub shell_name: Option<String>,
    /// Optionally provides a display string describing the version and variant of the shell.
    pub shell_product_display_str: Option<String>,
    /// Whether to run in maximal POSIX sh compatibility mode.
    pub sh_mode: bool,
    /// Whether to print verbose output.
    pub verbose: bool,
}

/// Represents an active shell function call.
#[derive(Clone, Debug)]
pub struct FunctionCall {
    /// The name of the function invoked.
    function_name: String,
    /// The definition of the invoked function.
    function_definition: Arc<brush_parser::ast::FunctionDefinition>,
}

lazy_static::lazy_static! {
    // NOTE: We have difficulty with xterm escape sequences going through rustyline;
    // so we compile a regex that can be used to strip them out.
    static ref PROMPT_XTERM_ESCAPE_SEQ_REGEX: fancy_regex::Regex = fancy_regex::Regex::new("\x1b][0-2];[^\x07]*\x07").unwrap();
}

impl Shell {
    /// Returns a new shell instance created with the given options.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use when creating the shell.
    pub async fn new(options: &CreateOptions) -> Result<Shell, error::Error> {
        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            traps: traps::TrapHandlerConfig::default(),
            open_files: openfiles::OpenFiles::default(),
            working_dir: std::env::current_dir()?,
            env: Self::initialize_vars(options)?,
            funcs: functions::FunctionEnv::default(),
            options: RuntimeOptions::defaults_from(options),
            jobs: jobs::JobManager::new(),
            aliases: HashMap::default(),
            last_exit_status: 0,
            positional_parameters: vec![],
            shell_name: options.shell_name.clone(),
            shell_product_display_str: options.shell_product_display_str.clone(),
            function_call_stack: VecDeque::new(),
            script_call_stack: VecDeque::new(),
            directory_stack: vec![],
            current_line_number: 0,
            completion_config: completion::Config::default(),
            builtins: builtins::get_default_builtins(options),
            depth: 0,
        };

        // TODO: Without this a script that sets extglob will fail because we
        // parse the entire script with the same settings.
        shell.options.extended_globbing = true;

        // Load profiles/configuration.
        shell
            .load_config(options, &shell.default_exec_params())
            .await?;

        Ok(shell)
    }

    fn initialize_vars(options: &CreateOptions) -> Result<ShellEnvironment, error::Error> {
        let mut env = ShellEnvironment::new();

        // Seed parameters from environment.
        for (k, v) in std::env::vars() {
            let mut var = ShellVariable::new(ShellValue::String(v));
            var.export();
            env.set_global(k, var)?;
        }

        // Set some additional ones.
        #[cfg(unix)]
        {
            let mut euid_var = ShellVariable::new(ShellValue::String(format!(
                "{}",
                uzers::get_effective_uid()
            )));
            euid_var.set_readonly();
            env.set_global("EUID", euid_var)?;
        }

        let mut random_var = ShellVariable::new(ShellValue::Random);
        random_var.hide_from_enumeration();
        random_var.treat_as_integer();
        env.set_global("RANDOM", random_var)?;

        env.set_global("IFS", ShellVariable::new(" \t\n".into()))?;
        env.set_global(
            "COMP_WORDBREAKS",
            ShellVariable::new(" \t\n\"\'><=;|&(:".into()),
        )?;

        let os_type = match std::env::consts::OS {
            "linux" => "linux-gnu",
            "windows" => "windows",
            _ => "unknown",
        };
        env.set_global("OSTYPE", ShellVariable::new(os_type.into()))?;

        // Set some defaults (if they're not already initialized).
        if !env.is_set("HISTFILE") {
            if let Some(home_dir) = Self::get_home_dir_with_env(&env) {
                let histfile = home_dir.join(".brush_history");
                env.set_global(
                    "HISTFILE",
                    ShellVariable::new(ShellValue::String(histfile.to_string_lossy().to_string())),
                )?;
            }
        }
        #[cfg(unix)]
        if !env.is_set("PATH") {
            env.set_global(
                "PATH",
                ShellVariable::new(
                    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
                ),
            )?;
        }

        if !options.sh_mode {
            if let Some(shell_name) = &options.shell_name {
                env.set_global("BASH", ShellVariable::new(shell_name.into()))?;
            }
            env.set_global(
                "BASH_VERSINFO",
                ShellVariable::new(ShellValue::indexed_array_from_slice(
                    ["5", "1", "1", "1", "release", "unknown"].as_slice(),
                )),
            )?;
        }

        Ok(env)
    }

    async fn load_config(
        &mut self,
        options: &CreateOptions,
        params: &ExecutionParameters,
    ) -> Result<(), error::Error> {
        if options.login {
            // --noprofile means skip this.
            if options.no_profile {
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
            self.source_if_exists(Path::new("/etc/profile"), params)
                .await?;
            if let Some(home_path) = self.get_home_dir() {
                if options.sh_mode {
                    self.source_if_exists(home_path.join(".profile").as_path(), params)
                        .await?;
                } else {
                    if !self
                        .source_if_exists(home_path.join(".bash_profile").as_path(), params)
                        .await?
                    {
                        if !self
                            .source_if_exists(home_path.join(".bash_login").as_path(), params)
                            .await?
                        {
                            self.source_if_exists(home_path.join(".profile").as_path(), params)
                                .await?;
                        }
                    }
                }
            }
        } else {
            if options.interactive {
                // --norc means skip this. Also skip in sh mode.
                if options.no_rc || options.sh_mode {
                    return Ok(());
                }

                //
                // For non-login interactive shells, load in this order:
                //
                //     /etc/bash.bashrc
                //     ~/.bashrc
                //
                self.source_if_exists(Path::new("/etc/bash.bashrc"), params)
                    .await?;
                if let Some(home_path) = self.get_home_dir() {
                    self.source_if_exists(home_path.join(".bashrc").as_path(), params)
                        .await?;
                    self.source_if_exists(home_path.join(".brushrc").as_path(), params)
                        .await?;
                }
            } else {
                let env_var_name = if options.sh_mode { "ENV" } else { "BASH_ENV" };

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
        path: &Path,
        params: &ExecutionParameters,
    ) -> Result<bool, error::Error> {
        if path.exists() {
            let args: Vec<String> = vec![];
            self.source(path, &args, params).await?;
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
    pub async fn source<S: AsRef<str>>(
        &mut self,
        path: &Path,
        args: &[S],
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        tracing::debug!("sourcing: {}", path.display());

        let path_to_open = self.get_absolute_path(path);

        let opened_file = std::fs::File::open(path_to_open)
            .map_err(|e| error::Error::FailedSourcingFile(path.to_owned(), e))?;

        let file_metadata = opened_file
            .metadata()
            .map_err(|e| error::Error::FailedSourcingFile(path.to_owned(), e))?;

        if file_metadata.is_dir() {
            return Err(error::Error::FailedSourcingFile(
                path.to_owned(),
                std::io::Error::new(std::io::ErrorKind::Other, error::Error::IsADirectory),
            ));
        }

        let source_info = brush_parser::SourceInfo {
            source: path.to_string_lossy().to_string(),
        };

        self.source_file(&opened_file, &source_info, args, params)
            .await
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `file` - The file to source.
    /// * `source_info` - Information about the source of the script.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    pub async fn source_file<S: AsRef<str>>(
        &mut self,
        file: &std::fs::File,
        source_info: &brush_parser::SourceInfo,
        args: &[S],
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut reader = std::io::BufReader::new(file);
        let mut parser =
            brush_parser::Parser::new(&mut reader, &self.parser_options(), source_info);

        tracing::debug!(target: "parse", "Parsing sourced file: {}", source_info.source);
        let parse_result = parser.parse(false);

        let mut other_positional_parameters = args.iter().map(|s| s.as_ref().to_owned()).collect();
        let mut other_shell_name = Some(source_info.source.clone());

        // TODO: Find a cleaner way to change args.
        std::mem::swap(&mut self.shell_name, &mut other_shell_name);
        std::mem::swap(
            &mut self.positional_parameters,
            &mut other_positional_parameters,
        );

        self.script_call_stack
            .push_front(source_info.source.clone());
        self.update_bash_source_var()?;

        let result = self
            .run_parsed_result(parse_result, source_info, params)
            .await;

        self.script_call_stack.pop_front();
        self.update_bash_source_var()?;

        // Restore.
        std::mem::swap(&mut self.shell_name, &mut other_shell_name);
        std::mem::swap(
            &mut self.positional_parameters,
            &mut other_positional_parameters,
        );

        result
    }

    /// Invokes a function defined in this shell, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to invoke.
    /// * `args` - The arguments to pass to the function.
    pub async fn invoke_function(&mut self, name: &str, args: &[&str]) -> Result<u8, error::Error> {
        let open_files = self.open_files.clone();
        let command_name = String::from(name);

        let func_registration = self
            .funcs
            .get(name)
            .ok_or_else(|| error::Error::FunctionNotFound(name.to_owned()))?;

        let func = func_registration.definition.clone();

        let context = commands::ExecutionContext {
            shell: self,
            command_name,
            open_files,
        };

        let command_args = args
            .iter()
            .map(|s| commands::CommandArg::String(String::from(*s)))
            .collect::<Vec<_>>();

        match commands::invoke_shell_function(func, context, &command_args).await? {
            commands::SpawnResult::SpawnedChild(_) => {
                error::unimp("child spawned from function invocation")
            }
            commands::SpawnResult::ImmediateExit(code) => Ok(code),
            commands::SpawnResult::ExitShell(code) => Ok(code),
            commands::SpawnResult::ReturnFromFunctionOrScript(code) => Ok(code),
            commands::SpawnResult::BreakLoop(_) | commands::SpawnResult::ContinueLoop(_) => {
                error::unimp("break or continue returned from function invocation")
            }
        }
    }

    /// Executes the given string as a shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute.
    /// * `params` - Execution parameters.
    pub async fn run_string(
        &mut self,
        command: String,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // TODO: Actually track line numbers; this is something of a hack, assuming each time
        // this function is invoked we are on the next line of the input. For one thing,
        // each string we run could be multiple lines.
        self.current_line_number += 1;

        let parse_result = self.parse_string(command);
        let source_info = brush_parser::SourceInfo {
            source: String::from("main"),
        };
        self.run_parsed_result(parse_result, &source_info, params)
            .await
    }

    /// Parses the given string as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to parse as a program.
    pub fn parse_string(
        &self,
        s: String,
    ) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
        parse_string_impl(s, self.parser_options())
    }

    /// Applies basic shell expansion to the provided string.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand.
    pub async fn basic_expand_string<S: AsRef<str>>(
        &mut self,
        s: S,
    ) -> Result<String, error::Error> {
        let result = expansion::basic_expand_str(self, s.as_ref()).await?;
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
        s: S,
    ) -> Result<Vec<String>, error::Error> {
        let result = expansion::full_expand_and_split_str(self, s.as_ref()).await?;
        Ok(result)
    }

    /// Returns the default execution parameters for this shell.
    pub fn default_exec_params(&self) -> ExecutionParameters {
        ExecutionParameters {
            open_files: self.open_files.clone(),
        }
    }

    /// Executes the given script file, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `script_path` - The path to the script file to execute.
    /// * `args` - The arguments to pass to the script as positional parameters.
    pub async fn run_script<S: AsRef<str>>(
        &mut self,
        script_path: &Path,
        args: &[S],
    ) -> Result<ExecutionResult, error::Error> {
        self.source(script_path, args, &self.default_exec_params())
            .await
    }

    async fn run_parsed_result(
        &mut self,
        parse_result: Result<brush_parser::ast::Program, brush_parser::ParseError>,
        source_info: &brush_parser::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut error_prefix = String::new();

        if !source_info.source.is_empty() {
            error_prefix = format!("{}: ", source_info.source);
        }

        let result = match parse_result {
            Ok(prog) => match self.run_program(prog, params).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("error: {:#}", e);
                    self.last_exit_status = 1;
                    ExecutionResult::new(1)
                }
            },
            Err(brush_parser::ParseError::ParsingNearToken(token_near_error)) => {
                let error_loc = &token_near_error.location().start;

                tracing::error!(
                    "{}syntax error near token `{}' (line {} col {})",
                    error_prefix,
                    token_near_error.to_str(),
                    error_loc.line,
                    error_loc.column,
                );
                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(brush_parser::ParseError::ParsingAtEndOfInput) => {
                tracing::error!("{}syntax error at end of input", error_prefix);

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(brush_parser::ParseError::Tokenizing { inner, position }) => {
                let mut error_message = error_prefix.clone();
                error_message.push_str(inner.to_string().as_str());

                if let Some(position) = position {
                    write!(
                        error_message,
                        " (detected near line {} column {})",
                        position.line, position.column
                    )?;
                }

                tracing::error!("{}", error_message);

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
        };

        Ok(result)
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

    fn default_prompt(&self) -> &'static str {
        if self.options.sh_mode {
            "$ "
        } else {
            "brush$ "
        }
    }

    /// Composes the shell's prompt, applying all appropriate expansions.
    pub async fn compose_prompt(&mut self) -> Result<String, error::Error> {
        // Retrieve the spec.
        let ps1 = self.parameter_or_default("PS1", self.default_prompt());

        // Expand it.
        let formatted_prompt = prompt::expand_prompt(self, ps1.as_ref())?;

        // NOTE: We're having difficulty with xterm escape sequences going through rustyline;
        // so we strip them here.
        let formatted_prompt = PROMPT_XTERM_ESCAPE_SEQ_REGEX
            .replace_all(formatted_prompt.as_str(), "")
            .to_string();

        // Now expand.
        let formatted_prompt = expansion::basic_expand_str(self, &formatted_prompt).await?;

        Ok(formatted_prompt)
    }

    /// Returns the exit status of the last command executed in this shell.
    pub fn last_result(&self) -> u8 {
        self.last_exit_status
    }

    fn parameter_or_default(&self, name: &str, default: &str) -> String {
        self.env.get(name).map_or_else(
            || default.to_owned(),
            |(_, s)| s.value().to_cow_string().to_string(),
        )
    }

    /// Returns a string representing the current `set`-style option flags set in the shell.
    pub(crate) fn current_option_flags(&self) -> String {
        let mut cs = vec![];

        for (x, y) in crate::namedoptions::SET_OPTIONS.iter() {
            if (y.getter)(&self.options) {
                cs.push(*x);
            }
        }

        // Sort the flags in a way that matches what bash does.
        cs.sort_by(|a, b| {
            if a == b {
                std::cmp::Ordering::Equal
            } else if *a == 's' {
                std::cmp::Ordering::Greater
            } else if *b == 's' {
                std::cmp::Ordering::Less
            } else if a.is_ascii_lowercase() && b.is_ascii_uppercase() {
                std::cmp::Ordering::Less
            } else if a.is_ascii_uppercase() && b.is_ascii_lowercase() {
                std::cmp::Ordering::Greater
            } else {
                a.cmp(b)
            }
        });

        cs.into_iter().collect()
    }

    /// Returns the options that should be used for parsing shell programs; reflects
    /// the current configuration state of the shell and may change over time.
    pub(crate) fn parser_options(&self) -> brush_parser::ParserOptions {
        brush_parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
            sh_mode: self.options.sh_mode,
            tilde_expansion: true,
        }
    }

    /// Returns whether or not the shell is actively executing in a shell function.
    pub(crate) fn in_function(&self) -> bool {
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
        self.function_call_stack.push_front(FunctionCall {
            function_name: name.to_owned(),
            function_definition: function_def.clone(),
        });
        self.env.push_scope(env::EnvironmentScope::Local);
        self.update_funcname_var()?;
        Ok(())
    }

    /// Updates the shell's internal tracking state to reflect that the shell
    /// has exited the top-most function on its call stack.
    pub(crate) fn leave_function(&mut self) -> Result<(), error::Error> {
        self.env.pop_scope(env::EnvironmentScope::Local)?;
        self.function_call_stack.pop_front();
        self.update_funcname_var()?;
        Ok(())
    }

    fn update_funcname_var(&mut self) -> Result<(), error::Error> {
        //
        // Fill out FUNCNAME[*]
        //
        let funcname_values = self
            .function_call_stack
            .iter()
            .map(|s| (None, s.function_name.clone()))
            .collect::<Vec<_>>();

        self.env.update_or_add(
            "FUNCNAME",
            variables::ShellValueLiteral::Array(variables::ArrayLiteral(funcname_values)),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        self.update_bash_source_var()
    }

    fn update_bash_source_var(&mut self) -> Result<(), error::Error> {
        //
        // Fill out BASH_SOURCE[*]
        //
        let source_values = if self.function_call_stack.is_empty() {
            self.script_call_stack
                .front()
                .map_or_else(Vec::new, |s| vec![(None, s.to_owned())])
        } else {
            self.function_call_stack
                .iter()
                .map(|s| (None, s.function_definition.source.clone()))
                .collect::<Vec<_>>()
        };

        self.env.update_or_add(
            "BASH_SOURCE",
            variables::ShellValueLiteral::Array(variables::ArrayLiteral(source_values)),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    /// Returns the path to the history file used by the shell, if one is set.
    pub fn get_history_file_path(&self) -> Option<PathBuf> {
        self.env.get("HISTFILE").map(|(_, var)| {
            let histfile_str: String = var.value().to_cow_string().to_string();
            PathBuf::from(histfile_str)
        })
    }

    /// Returns the number of the line being executed in the currently executing program.
    pub(crate) fn get_current_input_line_number(&self) -> u32 {
        self.current_line_number
    }

    /// Returns the current value of the IFS variable, or the default value if it is not set.
    pub(crate) fn get_ifs(&self) -> Cow<'_, str> {
        self.env.get("IFS").map_or_else(
            || Cow::Borrowed(" \t\n"),
            |(_, v)| v.value().to_cow_string(),
        )
    }

    /// Generates command completions for the shell.
    ///
    /// # Arguments
    ///
    /// * `input` - The input string to generate completions for.
    /// * `position` - The position in the input string to generate completions at.
    pub async fn get_completions(
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
    #[allow(clippy::manual_flatten)]
    pub(crate) fn find_executables_in_path(&self, required_glob_pattern: &str) -> Vec<PathBuf> {
        let is_executable = |path: &Path| path.executable();

        let mut executables = vec![];
        for dir_str in self.env.get_str("PATH").unwrap_or_default().split(':') {
            let pattern = std::format!("{dir_str}/{required_glob_pattern}");
            // TODO: Pass through quoting.
            if let Ok(entries) = patterns::Pattern::from(pattern).expand(
                &self.working_dir,
                self.options.extended_globbing,
                Some(&is_executable),
            ) {
                for entry in entries {
                    executables.push(PathBuf::from(entry));
                }
            }
        }

        executables
    }

    /// Gets the absolute form of the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to get the absolute form of.
    pub(crate) fn get_absolute_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_owned()
        } else {
            self.working_dir.join(path)
        }
    }

    /// Sets the shell's current working directory to the given path.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The path to set as the working directory.
    pub(crate) fn set_working_dir(&mut self, target_dir: &Path) -> Result<(), error::Error> {
        let abs_path = self.get_absolute_path(target_dir);

        match std::fs::metadata(&abs_path) {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::Error::NotADirectory(abs_path));
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // TODO: Don't canonicalize, just normalize.
        let cleaned_path = abs_path.canonicalize()?;

        let pwd = cleaned_path.to_string_lossy().to_string();

        // TODO: handle updating PWD
        self.working_dir = cleaned_path;
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

        Ok(())
    }

    /// Tilde-shortens the given string, replacing the user's home directory with a tilde.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to shorten.
    pub(crate) fn tilde_shorten(&self, s: String) -> String {
        if let Some(home_dir) = self.get_home_dir() {
            if let Some(stripped) = s.strip_prefix(home_dir.to_string_lossy().as_ref()) {
                return format!("~{stripped}");
            }
        }
        s
    }

    /// Returns the shell's current home directory, if available.
    pub(crate) fn get_home_dir(&self) -> Option<PathBuf> {
        Self::get_home_dir_with_env(&self.env)
    }

    fn get_home_dir_with_env(env: &ShellEnvironment) -> Option<PathBuf> {
        if let Some((_, home)) = env.get("HOME") {
            Some(PathBuf::from(home.value().to_cow_string().to_string()))
        } else {
            // HOME isn't set, so let's sort it out ourselves.
            users::get_user_home_dir()
        }
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard output stream using `write!` at al.
    pub fn stdout(&self) -> openfiles::OpenFile {
        self.open_files.files.get(&1).unwrap().try_dup().unwrap()
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard error stream using `write!` et al.
    pub fn stderr(&self) -> openfiles::OpenFile {
        self.open_files.files.get(&2).unwrap().try_dup().unwrap()
    }

    /// Outputs `set -x` style trace output for a command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to trace.
    pub(crate) fn trace_command<S: AsRef<str>>(&self, command: S) -> Result<(), std::io::Error> {
        // TODO: get prefix from PS4
        const DEFAULT_PREFIX: &str = "+ ";

        let mut prefix = DEFAULT_PREFIX.to_owned();

        let additional_depth = self.script_call_stack.len() + self.depth;
        if let Some(c) = prefix.chars().next() {
            for _ in 0..additional_depth {
                prefix.insert(0, c);
            }
        }

        writeln!(self.stderr(), "{prefix}{}", command.as_ref())
    }

    /// Returns the keywords that are reserved by the shell.
    pub(crate) fn get_keywords(&self) -> Vec<String> {
        if self.options.sh_mode {
            keywords::SH_MODE_KEYWORDS.iter().cloned().collect()
        } else {
            keywords::KEYWORDS.iter().cloned().collect()
        }
    }

    /// Checks for completed jobs in the shell, reporting any changes found.
    pub fn check_for_completed_jobs(&mut self) -> Result<(), error::Error> {
        let results = self.jobs.poll()?;

        if self.options.interactive {
            for (job, _result) in results {
                writeln!(self.stderr(), "{job}")?;
            }
        }

        Ok(())
    }

    /// Evaluate the given arithmetic expression, returning the result.
    pub async fn eval_arithmetic(
        &mut self,
        expr: brush_parser::ast::ArithmeticExpr,
    ) -> Result<i64, error::Error> {
        let result = expr.eval(self).await?;
        Ok(result)
    }
}

#[cached::proc_macro::cached(size = 32, result = true)]
fn parse_string_impl(
    s: String,
    parser_options: brush_parser::ParserOptions,
) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
    let mut reader = std::io::BufReader::new(s.as_bytes());
    let source_info = brush_parser::SourceInfo {
        source: String::from("main"),
    };
    let mut parser: brush_parser::Parser<&mut std::io::BufReader<&[u8]>> =
        brush_parser::Parser::new(&mut reader, &parser_options, &source_info);

    tracing::debug!(target: "parse", "Parsing string as program...");
    parser.parse(true)
}
