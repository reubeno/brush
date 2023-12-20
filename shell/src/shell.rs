use anyhow::{Context, Result};
use log::debug;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::expansion::WordExpander;
use crate::interp::{Execute, ExecutionParameters, ExecutionResult};
use crate::prompt::format_prompt_piece;

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
pub struct ShellVariable {
    pub value: String,
    pub exported: bool,
    pub readonly: bool,
}

#[derive(Default, Debug)]
pub struct ShellRuntimeOptions {
    //
    // Single-character options.
    //
    /// -a
    pub export_variables_on_modification: bool,
    /// -b
    pub notify_job_termination_immediately: bool,
    /// -e
    pub exit_on_nonzero_command_exit: bool,
    /// -f
    pub disable_filename_globbing: bool,
    /// -h
    pub remember_command_locations: bool,
    /// -k
    pub place_all_assignment_args_in_command_env: bool,
    /// -m
    pub enable_job_control: bool,
    /// -n
    pub do_not_execute_commands: bool,
    /// -p
    pub real_effective_uid_mismatch: bool,
    /// -t
    pub exit_after_one_command: bool,
    /// -u
    pub treat_unset_variables_as_error: bool,
    /// -v
    pub print_shell_input_lines: bool,
    /// -x
    pub print_commands_and_arguments: bool,
    /// -B
    pub perform_brace_expansion: bool,
    /// -C
    pub disallow_overwriting_regular_files_via_output_redirection: bool,
    /// -E
    pub shell_functions_inherit_err_trap: bool,
    /// -H
    pub enable_bang_style_history_substitution: bool,
    /// -P
    pub do_not_resolve_symlinks_when_changing_dir: bool,
    /// -T
    pub shell_functions_inherit_debug_and_return_traps: bool,

    //
    // Options set through -o.
    //
    /// 'emacs'
    pub emacs_mode: bool,
    /// 'history'
    pub enable_command_history: bool,
    /// 'ignoreeof'
    pub ignore_eof: bool,
    /// 'interactive-comments'
    pub allow_comments_in_interactive_commands: bool,
    /// 'pipefail'
    pub return_first_failure_from_pipeline: bool,
    /// 'posix'
    pub posix_mode: bool,
    /// 'vi'
    pub vi_mode: bool,

    //
    // Options set by the shell.
    //
    pub interactive: bool,
}

impl ShellRuntimeOptions {
    pub fn get_chars(&self) -> Vec<char> {
        let mut cs = vec![];
        if self.export_variables_on_modification {
            cs.push('a');
        }
        if self.notify_job_termination_immediately {
            cs.push('b');
        }
        if self.exit_on_nonzero_command_exit {
            cs.push('e');
        }
        if self.disable_filename_globbing {
            cs.push('f');
        }
        if self.remember_command_locations {
            cs.push('h');
        }
        if self.place_all_assignment_args_in_command_env {
            cs.push('k');
        }
        if self.enable_job_control {
            cs.push('m');
        }
        if self.do_not_execute_commands {
            cs.push('n');
        }
        if self.real_effective_uid_mismatch {
            cs.push('p');
        }
        if self.exit_after_one_command {
            cs.push('t');
        }
        if self.treat_unset_variables_as_error {
            cs.push('u');
        }
        if self.print_shell_input_lines {
            cs.push('v');
        }
        if self.print_commands_and_arguments {
            cs.push('x');
        }
        if self.perform_brace_expansion {
            cs.push('B');
        }
        if self.disallow_overwriting_regular_files_via_output_redirection {
            cs.push('C');
        }
        if self.shell_functions_inherit_err_trap {
            cs.push('E');
        }
        if self.enable_bang_style_history_substitution {
            cs.push('H');
        }
        if self.do_not_resolve_symlinks_when_changing_dir {
            cs.push('P');
        }
        if self.shell_functions_inherit_debug_and_return_traps {
            cs.push('T');
        }

        if self.interactive {
            cs.push('i');
        }

        cs
    }

    pub fn get_enabled_options(&self) -> Vec<&'static str> {
        let mut cs = vec![];
        if self.export_variables_on_modification {
            cs.push("allexport");
        }
        if self.perform_brace_expansion {
            cs.push("braceexpand");
        }
        if self.emacs_mode {
            cs.push("emacs");
        }
        if self.exit_on_nonzero_command_exit {
            cs.push("errexit");
        }
        if self.shell_functions_inherit_err_trap {
            cs.push("errtrace");
        }
        if self.shell_functions_inherit_debug_and_return_traps {
            cs.push("functrace");
        }
        if self.remember_command_locations {
            cs.push("hashall");
        }
        if self.enable_bang_style_history_substitution {
            cs.push("histexpand");
        }
        if self.enable_command_history {
            cs.push("history");
        }
        if self.ignore_eof {
            cs.push("ignoreeof");
        }
        if self.allow_comments_in_interactive_commands {
            cs.push("interactive_comments");
        }
        if self.place_all_assignment_args_in_command_env {
            cs.push("keyword");
        }
        if self.enable_job_control {
            cs.push("monitor");
        }
        if self.disallow_overwriting_regular_files_via_output_redirection {
            cs.push("noclobber");
        }
        if self.do_not_execute_commands {
            cs.push("noexec");
        }
        if self.disable_filename_globbing {
            cs.push("noglob");
        }
        if self.notify_job_termination_immediately {
            cs.push("notify");
        }
        if self.treat_unset_variables_as_error {
            cs.push("nounset");
        }
        if self.exit_after_one_command {
            cs.push("onecmd");
        }
        if self.do_not_resolve_symlinks_when_changing_dir {
            cs.push("physical");
        }
        if self.return_first_failure_from_pipeline {
            cs.push("pipefail");
        }
        if self.posix_mode {
            cs.push("posix");
        }
        if self.real_effective_uid_mismatch {
            cs.push("privileged");
        }
        if self.print_shell_input_lines {
            cs.push("verbose");
        }
        if self.vi_mode {
            cs.push("vi");
        }
        if self.print_commands_and_arguments {
            cs.push("xtrace");
        }

        cs
    }
}

#[derive(Debug)]
pub struct ShellCreateOptions {
    pub login: bool,
    pub interactive: bool,
    pub no_profile: bool,
    pub no_rc: bool,
    pub shell_name: Option<String>,
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
            options: ShellRuntimeOptions {
                interactive: options.interactive,
                ..Default::default()
            },
            aliases: Default::default(),
            last_exit_status: 0,
            positional_parameters: vec![],
            shell_name: options.shell_name.clone(),
        };

        // Load profiles/configuration.
        shell.load_config(options)?;

        Ok(shell)
    }

    pub fn set_var<S: AsRef<str>, T: AsRef<str>>(
        &mut self,
        name: S,
        value: T,
        exported: bool,
        readonly: bool,
    ) -> Result<()> {
        Self::set_var_in(&mut self.variables, name, value, exported, readonly)?;
        Ok(())
    }

    fn set_var_in<S: AsRef<str>, T: AsRef<str>>(
        vars: &mut HashMap<String, ShellVariable>,
        name: S,
        value: T,
        exported: bool,
        readonly: bool,
    ) -> Result<()> {
        vars.insert(
            name.as_ref().to_owned(),
            ShellVariable {
                value: value.as_ref().to_owned(),
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
            Self::set_var_in(&mut vars, k, v, true, false)?;
        }

        // Set some additional ones.
        Self::set_var_in(
            &mut vars,
            "EUID",
            format!("{}", uzers::get_effective_uid()),
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
        let mut parser = parser::Parser::new(&mut reader);
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
        let mut parser = parser::Parser::new(&mut reader);
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
            .map_or_else(|| default.to_owned(), |s| s.value.to_owned())
    }

    pub fn current_option_flags(&self) -> String {
        self.options.get_chars().into_iter().collect()
    }
}
