use faccess::PathExt;
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::context;
use crate::env::{EnvironmentLookup, EnvironmentScope, ShellEnvironment};
use crate::error;
use crate::expansion;
use crate::interp::{self, Execute, ExecutionParameters, ExecutionResult};
use crate::jobs;
use crate::openfiles;
use crate::options::RuntimeOptions;
use crate::prompt::expand_prompt;
use crate::variables::{self, ShellValue, ShellVariable};
use crate::{commands, patterns};
use crate::{completion, users};

pub struct Shell {
    pub open_files: openfiles::OpenFiles,
    pub working_dir: PathBuf,
    pub umask: u32,
    pub file_size_limit: u64,
    // TODO: traps
    pub env: ShellEnvironment,
    pub funcs: HashMap<String, Arc<parser::ast::FunctionDefinition>>,
    pub options: RuntimeOptions,
    pub jobs: jobs::JobManager,
    pub aliases: HashMap<String, String>,

    //
    // Additional state
    //
    pub last_exit_status: u8,

    // Track clone depth from main shell
    pub depth: usize,

    // Positional parameters ($1 and beyond)
    pub positional_parameters: Vec<String>,

    // Shell name
    pub shell_name: Option<String>,

    // Script call stack.
    pub script_call_stack: VecDeque<String>,

    // Function call stack.
    pub function_call_stack: VecDeque<FunctionCall>,

    // Directory stack used by pushd et al.
    pub directory_stack: Vec<PathBuf>,

    // Current line number being processed.
    pub current_line_number: u32,

    // Completion configuration.
    pub completion_config: completion::CompletionConfig,
}

impl Clone for Shell {
    fn clone(&self) -> Self {
        Self {
            open_files: self.open_files.clone(),
            working_dir: self.working_dir.clone(),
            umask: self.umask,
            file_size_limit: self.file_size_limit,
            env: self.env.clone(),
            funcs: self.funcs.clone(),
            options: self.options.clone(),
            jobs: jobs::JobManager::new(),
            aliases: self.aliases.clone(),
            last_exit_status: self.last_exit_status,
            positional_parameters: self.positional_parameters.clone(),
            shell_name: self.shell_name.clone(),
            function_call_stack: self.function_call_stack.clone(),
            script_call_stack: self.script_call_stack.clone(),
            directory_stack: self.directory_stack.clone(),
            current_line_number: self.current_line_number,
            completion_config: self.completion_config.clone(),
            depth: self.depth + 1,
        }
    }
}

#[derive(Debug, Default)]
pub struct CreateOptions {
    pub login: bool,
    pub interactive: bool,
    pub no_editing: bool,
    pub no_profile: bool,
    pub no_rc: bool,
    pub posix: bool,
    pub print_commands_and_arguments: bool,
    pub read_commands_from_stdin: bool,
    pub shell_name: Option<String>,
    pub sh_mode: bool,
    pub verbose: bool,
}

#[derive(Clone, Debug)]
pub struct FunctionCall {
    function_name: String,
    function_definition: Arc<parser::ast::FunctionDefinition>,
}

impl Shell {
    pub async fn new(options: &CreateOptions) -> Result<Shell, error::Error> {
        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            open_files: openfiles::OpenFiles::default(),
            working_dir: std::env::current_dir()?,
            umask: Default::default(),           // TODO: populate umask
            file_size_limit: Default::default(), // TODO: populate file size limit
            env: Self::initialize_vars(options),
            funcs: HashMap::default(),
            options: RuntimeOptions::defaults_from(options),
            jobs: jobs::JobManager::new(),
            aliases: HashMap::default(),
            last_exit_status: 0,
            positional_parameters: vec![],
            shell_name: options.shell_name.clone(),
            function_call_stack: VecDeque::new(),
            script_call_stack: VecDeque::new(),
            directory_stack: vec![],
            current_line_number: 0,
            completion_config: completion::CompletionConfig::default(),
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

    fn initialize_vars(options: &CreateOptions) -> ShellEnvironment {
        let mut env = ShellEnvironment::new();

        // Seed parameters from environment.
        for (k, v) in std::env::vars() {
            let mut var = ShellVariable::new(ShellValue::String(v));
            var.export();
            env.set_global(k, var);
        }

        // Set some additional ones.
        #[cfg(unix)]
        {
            let mut euid_var = ShellVariable::new(ShellValue::String(format!(
                "{}",
                uzers::get_effective_uid()
            )));
            euid_var.set_readonly();
            env.set_global("EUID", euid_var);
        }

        let mut random_var = ShellVariable::new(ShellValue::Random);
        random_var.hide_from_enumeration();
        env.set_global("RANDOM", random_var);

        env.set_global("IFS", ShellVariable::new(" \t\n".into()));
        env.set_global(
            "COMP_WORDBREAKS",
            ShellVariable::new(" \t\n\"\'><=;|&(:".into()),
        );

        let os_type = match std::env::consts::OS {
            "linux" => "linux-gnu",
            "windows" => "windows",
            _ => "unknown",
        };
        env.set_global("OSTYPE", ShellVariable::new(os_type.into()));

        // Set some defaults (if they're not already initialized).
        if !env.is_set("HISTFILE") {
            if let Some(home_dir) = Self::get_home_dir_with_env(&env) {
                let histfile = home_dir.join(".brush_history");
                env.set_global(
                    "HISTFILE",
                    ShellVariable::new(ShellValue::String(histfile.to_string_lossy().to_string())),
                );
            }
        }
        #[cfg(unix)]
        if !env.is_set("PATH") {
            env.set_global(
                "PATH",
                ShellVariable::new(
                    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
                ),
            );
        }

        if !options.sh_mode {
            if let Some(shell_name) = &options.shell_name {
                env.set_global("BASH", ShellVariable::new(shell_name.into()));
            }
            env.set_global(
                "BASH_VERSINFO",
                ShellVariable::new(ShellValue::indexed_array_from_slice(
                    ["5", "1", "1", "1", "release", "unknown"].as_slice(),
                )),
            );
        }

        env
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
            log::debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    pub async fn source<S: AsRef<str>>(
        &mut self,
        path: &Path,
        args: &[S],
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        log::debug!("sourcing: {}", path.display());

        let opened_file = std::fs::File::open(path)
            .map_err(|e| error::Error::FailedSourcingFile(path.to_owned(), e))?;

        let file_metadata = opened_file
            .metadata()
            .map_err(|e| error::Error::FailedSourcingFile(path.to_owned(), e))?;

        if file_metadata.is_dir() {
            return Err(error::Error::FailedSourcingFile(
                path.to_owned(),
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    anyhow::anyhow!("path is a directory"),
                ),
            ));
        }

        let source_info = parser::SourceInfo {
            source: path.to_string_lossy().to_string(),
        };

        self.source_file(&opened_file, &source_info, args, params)
            .await
    }

    pub async fn source_file<S: AsRef<str>>(
        &mut self,
        file: &std::fs::File,
        source_info: &parser::SourceInfo,
        args: &[S],
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut reader = std::io::BufReader::new(file);
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options(), source_info);
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

    pub async fn invoke_function(&mut self, name: &str, args: &[&str]) -> Result<u8, error::Error> {
        let open_files = self.open_files.clone();
        let command_name = String::from(name);

        let func = self
            .funcs
            .get(name)
            .ok_or_else(|| error::Error::FunctionNotFound(name.to_owned()))?
            .to_owned();

        let context = context::CommandExecutionContext {
            shell: self,
            command_name,
            open_files,
        };

        let command_args = args
            .iter()
            .map(|s| commands::CommandArg::String(String::from(*s)))
            .collect::<Vec<_>>();

        match interp::invoke_shell_function(func, context, &command_args, vec![]).await? {
            interp::SpawnResult::SpawnedChild(_) => {
                error::unimp("child spawned from function invocation")
            }
            interp::SpawnResult::ImmediateExit(code) => Ok(code),
            interp::SpawnResult::ExitShell(code) => Ok(code),
            interp::SpawnResult::ReturnFromFunctionOrScript(code) => Ok(code),
            interp::SpawnResult::BreakLoop(_) | interp::SpawnResult::ContinueLoop(_) => {
                error::unimp("break or continue returned from function invocation")
            }
        }
    }

    pub async fn run_string(
        &mut self,
        command: &str,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // TODO: Actually track line numbers; this is something of a hack, assuming each time
        // this function is invoked we are on the next line of the input. For one thing,
        // each string we run could be multiple lines.
        self.current_line_number += 1;

        let parse_result = self.parse_string(command);
        let source_info = parser::SourceInfo {
            source: String::from("main"),
        };
        self.run_parsed_result(parse_result, &source_info, params)
            .await
    }

    pub fn parse_string<S: AsRef<str>>(
        &self,
        s: S,
    ) -> Result<parser::ast::Program, parser::ParseError> {
        let mut reader = std::io::BufReader::new(s.as_ref().as_bytes());
        let source_info = parser::SourceInfo {
            source: String::from("main"),
        };
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options(), &source_info);
        parser.parse(true)
    }

    pub async fn basic_expand_string<S: AsRef<str>>(
        &mut self,
        s: S,
    ) -> Result<String, error::Error> {
        let result = expansion::basic_expand_str(self, s.as_ref()).await?;
        Ok(result)
    }

    pub async fn full_expand_and_split_string<S: AsRef<str>>(
        &mut self,
        s: S,
    ) -> Result<Vec<String>, error::Error> {
        let result = expansion::full_expand_and_split_str(self, s.as_ref()).await?;
        Ok(result)
    }

    pub fn default_exec_params(&self) -> ExecutionParameters {
        ExecutionParameters {
            open_files: self.open_files.clone(),
        }
    }

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
        parse_result: Result<parser::ast::Program, parser::ParseError>,
        source_info: &parser::SourceInfo,
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
                    log::error!("error: {:#}", e);
                    self.last_exit_status = 1;
                    ExecutionResult::new(1)
                }
            },
            Err(parser::ParseError::ParsingNearToken(token_near_error)) => {
                let error_loc = &token_near_error.location().start;

                log::error!(
                    "{}syntax error near token `{}' (line {} col {})",
                    error_prefix,
                    token_near_error.to_str(),
                    error_loc.line,
                    error_loc.column,
                );
                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(parser::ParseError::ParsingAtEndOfInput) => {
                log::error!("{}syntax error at end of input", error_prefix);

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(parser::ParseError::Tokenizing { inner, position }) => {
                let mut error_message = error_prefix.clone();
                error_message.push_str(inner.to_string().as_str());

                if let Some(position) = position {
                    error_message.push_str(&format!(
                        " (detected near line {} column {})",
                        position.line, position.column
                    ));
                }

                log::error!("{}", error_message);

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
        };

        Ok(result)
    }

    pub async fn run_program(
        &mut self,
        program: parser::ast::Program,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        program.execute(self, params).await
    }

    pub async fn compose_prompt(&mut self) -> Result<String, error::Error> {
        const DEFAULT_PROMPT: &str = "$ ";

        // Retrieve the spec.
        let ps1 = self.parameter_or_default("PS1", DEFAULT_PROMPT);

        // Expand it.
        let formatted_prompt = expand_prompt(self, ps1.as_ref())?;

        // NOTE: We're having difficulty with xterm escape sequences going through rustyline;
        // so we strip them here.
        let re = fancy_regex::Regex::new("\x1b][0-2];[^\x07]*\x07")?;
        let formatted_prompt = re.replace_all(formatted_prompt.as_str(), "").to_string();

        // Now expand.
        let formatted_prompt = expansion::basic_expand_str(self, &formatted_prompt).await?;

        Ok(formatted_prompt)
    }

    pub fn last_result(&self) -> u8 {
        self.last_exit_status
    }

    fn parameter_or_default(&self, name: &str, default: &str) -> String {
        self.env.get(name).map_or_else(
            || default.to_owned(),
            |s| s.value().to_cow_string().to_string(),
        )
    }

    pub fn current_option_flags(&self) -> String {
        let mut cs = vec![];

        for (x, y) in crate::namedoptions::SET_OPTIONS.iter() {
            if (y.getter)(self) {
                cs.push(*x);
            }
        }

        cs.into_iter().collect()
    }

    pub fn parser_options(&self) -> parser::ParserOptions {
        parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
            sh_mode: self.options.sh_mode,
            tilde_expansion: true,
        }
    }

    pub fn in_function(&self) -> bool {
        !self.function_call_stack.is_empty()
    }

    pub fn enter_function(
        &mut self,
        name: &str,
        function_def: &Arc<parser::ast::FunctionDefinition>,
    ) -> Result<(), error::Error> {
        self.function_call_stack.push_front(FunctionCall {
            function_name: name.to_owned(),
            function_definition: function_def.clone(),
        });
        self.env.push_locals();
        self.update_funcname_var()?;
        Ok(())
    }

    pub fn leave_function(&mut self) -> Result<(), error::Error> {
        self.env.pop_locals();
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

    pub fn get_history_file_path(&self) -> Option<PathBuf> {
        self.env.get("HISTFILE").map(|var| {
            let histfile_str: String = var.value().to_cow_string().to_string();
            PathBuf::from(histfile_str)
        })
    }

    pub fn get_current_input_line_number(&self) -> u32 {
        self.current_line_number
    }

    pub fn get_ifs(&self) -> Cow<'_, str> {
        self.env
            .get("IFS")
            .map_or_else(|| Cow::Borrowed(" \t\n"), |v| v.value().to_cow_string())
    }

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

    #[allow(clippy::manual_flatten)]
    pub fn find_executables_in_path(&self, required_glob_pattern: &str) -> Vec<PathBuf> {
        let mut executables = vec![];
        for dir_str in self.env.get_str("PATH").unwrap_or_default().split(':') {
            let pattern = std::format!("{dir_str}/{required_glob_pattern}");
            // TODO: Pass through quoting.
            if let Ok(entries) = patterns::Pattern::from(pattern)
                .expand(&self.working_dir, self.options.extended_globbing)
            {
                for entry in entries {
                    let path = Path::new(&entry);
                    if path.executable() {
                        executables.push(path.to_path_buf());
                    }
                }
            }
        }

        executables
    }

    pub fn set_working_dir(&mut self, target_dir: &Path) -> Result<(), error::Error> {
        let abs_path = if target_dir.is_absolute() {
            PathBuf::from(target_dir)
        } else {
            self.working_dir.join(target_dir)
        };

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

    pub fn tilde_shorten(&self, s: String) -> String {
        if let Some(home_dir) = self.get_home_dir() {
            if let Some(stripped) = s.strip_prefix(home_dir.to_string_lossy().as_ref()) {
                return format!("~{stripped}");
            }
        }
        s
    }

    pub fn get_home_dir(&self) -> Option<PathBuf> {
        Self::get_home_dir_with_env(&self.env)
    }

    fn get_home_dir_with_env(env: &ShellEnvironment) -> Option<PathBuf> {
        if let Some(home) = env.get("HOME") {
            Some(PathBuf::from(home.value().to_cow_string().to_string()))
        } else {
            // HOME isn't set, so let's sort it out ourselves.
            users::get_user_home_dir()
        }
    }

    pub fn stdout(&self) -> openfiles::OpenFile {
        self.open_files.files.get(&1).unwrap().try_dup().unwrap()
    }

    pub fn stderr(&self) -> openfiles::OpenFile {
        self.open_files.files.get(&2).unwrap().try_dup().unwrap()
    }

    pub fn trace_command<S: AsRef<str>>(&self, command: S) -> Result<(), std::io::Error> {
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
}
