use anyhow::{Context, Result};
use log::debug;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::ShellEnvironment;
use crate::expansion::WordExpander;
use crate::interp::{Execute, ExecutionParameters, ExecutionResult};
use crate::options::RuntimeOptions;
use crate::prompt::expand_prompt;
use crate::variables;

pub struct Shell {
    // TODO: open files
    pub working_dir: PathBuf,
    pub umask: u32,
    pub file_size_limit: u64,
    // TODO: traps
    pub env: ShellEnvironment,
    pub funcs: HashMap<String, ShellFunction>,
    pub options: RuntimeOptions,
    pub background_jobs: Vec<tokio::task::JoinHandle<Result<ExecutionResult>>>,
    // TODO: async lists
    pub aliases: HashMap<String, String>,

    //
    // Additional state
    //
    pub last_exit_status: u8,

    // Positional parameters ($1 and beyond)
    pub positional_parameters: Vec<String>,

    // Shell name
    pub shell_name: Option<String>,

    // Function call stack.
    pub function_call_depth: u32,
}

impl Clone for Shell {
    fn clone(&self) -> Self {
        Self {
            working_dir: self.working_dir.clone(),
            umask: self.umask,
            file_size_limit: self.file_size_limit,
            env: self.env.clone(),
            funcs: self.funcs.clone(),
            options: self.options.clone(),
            background_jobs: vec![],
            aliases: self.aliases.clone(),
            last_exit_status: self.last_exit_status,
            positional_parameters: self.positional_parameters.clone(),
            shell_name: self.shell_name.clone(),
            function_call_depth: self.function_call_depth,
        }
    }
}

#[derive(Debug)]
pub struct CreateOptions {
    pub login: bool,
    pub interactive: bool,
    pub no_editing: bool,
    pub no_profile: bool,
    pub no_rc: bool,
    pub posix: bool,
    pub print_commands_and_arguments: bool,
    pub shell_name: Option<String>,
    pub verbose: bool,
}

type ShellFunction = parser::ast::FunctionDefinition;

#[derive(Debug)]
pub struct FunctionCall {}

pub enum ProgramOrigin {
    File(PathBuf),
    String,
}

impl ProgramOrigin {
    fn get_name(&self) -> String {
        match self {
            ProgramOrigin::File(path) => path.to_string_lossy().to_string(),
            ProgramOrigin::String => "<string>".to_owned(),
        }
    }
}

impl std::fmt::Display for ProgramOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_name())
    }
}

impl Shell {
    pub async fn new(options: &CreateOptions) -> Result<Shell> {
        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            working_dir: std::env::current_dir()?,
            umask: Default::default(),           // TODO: populate umask
            file_size_limit: Default::default(), // TODO: populate file size limit
            env: Self::initialize_vars(options),
            funcs: HashMap::default(),
            options: RuntimeOptions::defaults_from(options),
            background_jobs: vec![],
            aliases: HashMap::default(),
            last_exit_status: 0,
            positional_parameters: vec![],
            shell_name: options.shell_name.clone(),
            function_call_depth: 0,
        };

        // Load profiles/configuration.
        shell.load_config(options).await?;

        Ok(shell)
    }

    fn initialize_vars(options: &CreateOptions) -> ShellEnvironment {
        let mut env = ShellEnvironment::new();

        // Seed parameters from environment.
        for (k, v) in std::env::vars() {
            env.set_global(k, v.as_str()).export();
        }

        // Set some additional ones.
        env.set_global("EUID", format!("{}", uzers::get_effective_uid()).as_str())
            .readonly = true;
        env.set_global("RANDOM", variables::ShellValue::Random)
            .enumerable = false;

        // TODO: don't set these in sh mode
        if let Some(shell_name) = &options.shell_name {
            env.set_global("BASH", shell_name);
        }
        env.set_global(
            "BASH_VERSINFO",
            ["5", "1", "1", "1", "release", "unknown"].as_slice(),
        );

        env
    }

    async fn load_config(&mut self, options: &CreateOptions) -> Result<()> {
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
            self.source_if_exists(Path::new("/etc/profile")).await?;
            if let Ok(home_path) = std::env::var("HOME") {
                if !self
                    .source_if_exists(Path::new(&home_path).join(".bash_profile").as_path())
                    .await?
                {
                    if !self
                        .source_if_exists(Path::new(&home_path).join(".bash_login").as_path())
                        .await?
                    {
                        self.source_if_exists(Path::new(&home_path).join(".profile").as_path())
                            .await?;
                    }
                }
            }
        } else {
            if options.interactive {
                // --norc means skip this.
                if options.no_rc {
                    return Ok(());
                }

                //
                // For non-login interactive shells, load in this order:
                //
                //     /etc/bash.bashrc
                //     ~/.bashrc
                //
                self.source_if_exists(Path::new("/etc/bash.bashrc")).await?;
                if let Ok(home_path) = std::env::var("HOME") {
                    self.source_if_exists(Path::new(&home_path).join(".bashrc").as_path())
                        .await?;
                }
            } else {
                if self.env.is_set("BASH_ENV") {
                    //
                    // TODO: look at $BASH_ENV; source its expansion if that file exists
                    //
                    todo!("UNIMPLEMENTED: load config from $BASH_ENV for non-interactive, non-login shell")
                }
            }
        }

        Ok(())
    }

    async fn source_if_exists(&mut self, path: &Path) -> Result<bool> {
        if path.exists() {
            let args: Vec<String> = vec![];
            self.source(path, &args).await?;
            Ok(true)
        } else {
            debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    pub async fn source<S: AsRef<str>>(
        &mut self,
        path: &Path,
        args: &[S],
    ) -> Result<ExecutionResult> {
        debug!("sourcing: {}", path.display());

        let opened_file = std::fs::File::open(path).context(path.to_string_lossy().to_string())?;
        if opened_file.metadata()?.is_dir() {
            return Err(anyhow::anyhow!(
                "{}: path is a directory",
                path.to_string_lossy().to_string()
            ));
        }

        let origin = ProgramOrigin::File(path.to_owned());

        self.source_file(&opened_file, &origin, args).await
    }

    pub async fn source_file<S: AsRef<str>>(
        &mut self,
        file: &std::fs::File,
        origin: &ProgramOrigin,
        args: &[S],
    ) -> Result<ExecutionResult> {
        let mut reader = std::io::BufReader::new(file);
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options());
        let parse_result = parser.parse(false)?;

        // TODO: Find a cleaner way to change args.
        let orig_shell_name = self.shell_name.take();
        let orig_params = self.positional_parameters.clone();
        self.shell_name = Some(origin.get_name());
        self.positional_parameters = vec![];

        // TODO: handle args
        if !args.is_empty() {
            log::error!("UNIMPLEMENTED: source built-in invoked with args: {origin}",);
        }

        let result = self.run_parsed_result(&parse_result, origin, false).await;

        // Restore.
        self.shell_name = orig_shell_name;
        self.positional_parameters = orig_params;

        result
    }

    pub async fn run_string(
        &mut self,
        command: &str,
        capture_output: bool,
    ) -> Result<ExecutionResult> {
        let mut reader = std::io::BufReader::new(command.as_bytes());
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options());
        let parse_result = parser.parse(true)?;

        self.run_parsed_result(&parse_result, &ProgramOrigin::String, capture_output)
            .await
    }

    pub async fn run_script<S: AsRef<str>>(
        &mut self,
        script_path: &Path,
        args: &[S],
    ) -> Result<ExecutionResult> {
        self.source(script_path, args).await
    }

    async fn run_parsed_result(
        &mut self,
        parse_result: &parser::ParseResult,
        origin: &ProgramOrigin,
        capture_output: bool,
    ) -> Result<ExecutionResult> {
        let mut error_prefix = String::new();

        if let ProgramOrigin::File(file_path) = origin {
            error_prefix = format!("{}: ", file_path.display());
        }

        let result = match parse_result {
            parser::ParseResult::Program(prog) => self.run_program(prog, capture_output).await?,
            parser::ParseResult::ParseError(token_near_error) => {
                if let Some(token_near_error) = &token_near_error {
                    let error_loc = &token_near_error.location().start;

                    log::error!(
                        "{}syntax error near token `{}' (line {} col {})",
                        error_prefix,
                        token_near_error.to_str(),
                        error_loc.line,
                        error_loc.column,
                    );
                } else {
                    log::error!("{}syntax error at end of input", error_prefix);
                }

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            parser::ParseResult::TokenizerError { message, position } => {
                let mut error_message = error_prefix.clone();
                error_message.push_str(message);

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
        program: &parser::ast::Program,
        capture_output: bool,
    ) -> Result<ExecutionResult> {
        program
            .execute(self, &ExecutionParameters { capture_output })
            .await
    }

    pub async fn compose_prompt(&mut self) -> Result<String> {
        const DEFAULT_PROMPT: &str = "$ ";

        // Retrieve the spec.
        let ps1 = self.parameter_or_default("PS1", DEFAULT_PROMPT);

        // Expand it.
        let formatted_prompt = expand_prompt(self, ps1.as_str())?;

        // NOTE: We're having difficulty with xterm escape sequences going through rustyline;
        // so we strip them here.
        let re = regex::Regex::new("\x1b][0-2];[^\x07]*\x07")?;
        let formatted_prompt = re.replace_all(formatted_prompt.as_str(), "").to_string();

        // Now expand.
        let mut expander = WordExpander::new(self);
        let formatted_prompt = expander.expand(formatted_prompt.as_str()).await?;

        Ok(formatted_prompt)
    }

    pub fn last_result(&self) -> u8 {
        self.last_exit_status
    }

    fn parameter_or_default(&self, name: &str, default: &str) -> String {
        self.env
            .get(name)
            .map_or_else(|| default.to_owned(), |s| String::from(&s.value))
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

    fn parser_options(&self) -> parser::ParserOptions {
        parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
        }
    }

    pub fn in_function(&self) -> bool {
        self.function_call_depth > 0
    }

    pub fn enter_function(&mut self) {
        self.function_call_depth += 1;
        self.env.push_locals();
    }

    pub fn leave_function(&mut self) {
        self.env.pop_locals();
        self.function_call_depth -= 1;
    }
}
