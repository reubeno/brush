use anyhow::{Context, Result};
use log::debug;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::expansion::WordExpander;
use crate::interp::{Execute, ExecutionParameters, ExecutionResult};
use crate::options::ShellRuntimeOptions;
use crate::prompt::format_prompt_piece;
use crate::variables::{ShellValue, ShellVariable};

#[derive(Debug)]
pub struct Shell {
    // TODO: open files
    pub working_dir: PathBuf,
    pub umask: u32,
    pub file_size_limit: u64,
    // TODO: traps
    pub variables: HashMap<String, ShellVariable>,
    pub funcs: HashMap<String, ShellFunction>,
    pub options: ShellRuntimeOptions,
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
}

#[derive(Debug)]
pub struct ShellCreateOptions {
    pub login: bool,
    pub interactive: bool,
    pub no_editing: bool,
    pub no_profile: bool,
    pub no_rc: bool,
    pub posix: bool,
    pub shell_name: Option<String>,
    pub verbose: bool,
}

type ShellFunction = parser::ast::FunctionDefinition;

enum ProgramOrigin {
    File(PathBuf),
    String,
}

impl Shell {
    pub fn new(options: &ShellCreateOptions) -> Result<Shell> {
        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            working_dir: std::env::current_dir()?,
            umask: Default::default(),           // TODO: populate umask
            file_size_limit: Default::default(), // TODO: populate file size limit
            variables: Self::initialize_vars()?,
            funcs: Default::default(),
            options: ShellRuntimeOptions::defaults_from(options),
            aliases: Default::default(),
            last_exit_status: 0,
            positional_parameters: vec![],
            shell_name: options.shell_name.clone(),
        };

        // Load profiles/configuration.
        shell.load_config(options)?;

        Ok(shell)
    }

    pub fn set_var<N: AsRef<str>, V: Into<ShellValue>>(
        &mut self,
        name: N,
        value: V,
        exported: bool,
        readonly: bool,
    ) -> Result<()> {
        Self::set_var_in(&mut self.variables, name, value, exported, readonly)?;
        Ok(())
    }

    // pub fn set_var<S: AsRef<str>, T: AsRef<str>>(
    //     &mut self,
    //     name: S,
    //     value: T,
    //     exported: bool,
    //     readonly: bool,
    // ) -> Result<()> {
    //     Self::set_var_in(&mut self.variables, name, value, exported, readonly)?;
    //     Ok(())
    // }

    fn set_var_in<N: AsRef<str>, V: Into<ShellValue>>(
        vars: &mut HashMap<String, ShellVariable>,
        name: N,
        value: V,
        exported: bool,
        readonly: bool,
    ) -> Result<()> {
        vars.insert(
            name.as_ref().to_owned(),
            ShellVariable {
                value: value.into(),
                exported,
                readonly,
            },
        );

        Ok(())
    }

    fn initialize_vars() -> Result<HashMap<String, ShellVariable>> {
        let mut vars = HashMap::new();

        // Seed parameters from environment.
        for (k, v) in std::env::vars() {
            Self::set_var_in(&mut vars, k, v.as_str(), true, false)?;
        }

        // Set some additional ones.
        Self::set_var_in(
            &mut vars,
            "EUID",
            format!("{}", uzers::get_effective_uid()).as_str(),
            false,
            true,
        )?;

        Ok(vars)
    }

    fn load_config(&mut self, options: &ShellCreateOptions) -> Result<()> {
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
            self.source_if_exists(Path::new("/etc/profile"))?;
            if let Ok(home_path) = std::env::var("HOME") {
                if !self.source_if_exists(Path::new(&home_path).join(".bash_profile").as_path())? {
                    if !self
                        .source_if_exists(Path::new(&home_path).join(".bash_login").as_path())?
                    {
                        self.source_if_exists(Path::new(&home_path).join(".profile").as_path())?;
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
                self.source_if_exists(Path::new("/etc/bash.bashrc"))?;
                if let Ok(home_path) = std::env::var("HOME") {
                    self.source_if_exists(Path::new(&home_path).join(".bashrc").as_path())?;
                }
            } else {
                if self.variables.contains_key("BASH_ENV") {
                    //
                    // TODO: look at $BASH_ENV; source its expansion if that file exists
                    //
                    todo!("load config from $BASH_ENV for non-interactive, non-login shell")
                }
            }
        }

        Ok(())
    }

    fn source_if_exists(&mut self, path: &Path) -> Result<bool> {
        if path.exists() {
            let args: Vec<String> = vec![];
            self.source(path, &args)?;
            Ok(true)
        } else {
            debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    pub fn source<S: AsRef<str>>(&mut self, path: &Path, args: &[S]) -> Result<ExecutionResult> {
        debug!("sourcing: {}", path.display());

        let opened_file = std::fs::File::open(path).context(path.to_string_lossy().to_string())?;
        if !opened_file.metadata()?.is_file() {
            return Err(anyhow::anyhow!(
                "{}: path is a directory",
                path.to_string_lossy().to_string()
            ));
        }

        let mut reader = std::io::BufReader::new(opened_file);
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options());
        let parse_result = parser.parse(false)?;

        // TODO: Find a cleaner way to change args.
        let orig_shell_name = self.shell_name.take();
        let orig_params = self.positional_parameters.clone();
        self.shell_name = Some(path.to_string_lossy().to_string());
        self.positional_parameters = vec![];

        // TODO: handle args
        if !args.is_empty() {
            log::error!(
                "UNIMPLEMENTED: source built-in invoked with args: {:?}",
                path
            );
        }

        let result =
            self.run_parsed_result(&parse_result, &ProgramOrigin::File(path.to_owned()), false);

        // Restore.
        self.shell_name = orig_shell_name;
        self.positional_parameters = orig_params;

        result
    }

    pub fn run_string(&mut self, command: &str, capture_output: bool) -> Result<ExecutionResult> {
        let mut reader = std::io::BufReader::new(command.as_bytes());
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options());
        let parse_result = parser.parse(true)?;

        self.run_parsed_result(&parse_result, &ProgramOrigin::String, capture_output)
    }

    pub fn run_script<S: AsRef<str>>(
        &mut self,
        script_path: &Path,
        args: &[S],
    ) -> Result<ExecutionResult> {
        self.source(script_path, args)
    }

    fn run_parsed_result(
        &mut self,
        parse_result: &parser::ParseResult,
        origin: &ProgramOrigin,
        capture_output: bool,
    ) -> Result<ExecutionResult> {
        let mut error_prefix = "".to_owned();

        if let ProgramOrigin::File(file_path) = origin {
            error_prefix = format!("{}: ", file_path.display());
        }

        let result = match parse_result {
            parser::ParseResult::Program(prog) => self.run_program(prog, capture_output)?,
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

    pub fn run_program(
        &mut self,
        program: &parser::ast::Program,
        capture_output: bool,
    ) -> Result<ExecutionResult> {
        program.execute(self, &ExecutionParameters { capture_output })
    }

    pub fn compose_prompt(&mut self) -> Result<String> {
        const DEFAULT_PROMPT: &str = "$ ";

        let ps1 = self.parameter_or_default("PS1", DEFAULT_PROMPT);
        let prompt_pieces = parser::prompt::parse_prompt(&ps1)?;

        let formatted_prompt = prompt_pieces
            .iter()
            .map(|p| format_prompt_piece(self, p))
            .collect::<Result<Vec<_>>>()?
            .join("");

        // NOTE: We're having difficulty with xterm escape sequences going through rustyline;
        // so we strip them here.
        let re = regex::Regex::new("\x1b][0-2];[^\x07]*\x07")?;
        let formatted_prompt = re.replace_all(formatted_prompt.as_str(), "").to_string();

        // Now expand.
        let mut expander = WordExpander::new(self);
        let formatted_prompt = expander.expand(formatted_prompt.as_str())?;

        Ok(formatted_prompt)
    }

    pub fn last_result(&self) -> u8 {
        self.last_exit_status
    }

    fn parameter_or_default(&self, name: &str, default: &str) -> String {
        self.variables
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
}
