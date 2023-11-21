use anyhow::Result;
use log::debug;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::interp::Execute;
use crate::prompt::format_prompt_piece;

#[derive(Debug)]
pub struct Shell {
    // TODO: open files
    pub working_dir: PathBuf,
    pub umask: u32,
    pub file_size_limit: u64,
    // TODO: traps
    pub parameters: HashMap<String, ShellVariable>,
    pub funcs: HashMap<String, ShellFunction>,
    pub options: ShellRuntimeOptions,
    // TODO: async lists
    pub aliases: HashMap<String, String>,

    //
    // Additional state
    //
    pub last_pipeline_exit_status: u32,
}

#[derive(Debug)]
pub struct ShellVariable {
    pub value: String,
    pub exported: bool,
    pub readonly: bool,
}

#[derive(Debug)]
pub struct ShellRuntimeOptions {
    //
    // Single-character options.
    //
    /// -a
    export_variables_on_modification: bool,
    /// -b
    notify_job_termination_immediately: bool,
    /// -e
    exit_on_nonzero_command_exit: bool,
    /// -f
    disable_filename_globbing: bool,
    /// -h
    remember_command_locations: bool,
    /// -k
    place_all_assignment_args_in_command_env: bool,
    /// -m
    enable_job_control: bool,
    /// -n
    do_not_execute_commands: bool,
    /// -p
    real_effective_uid_mismatch: bool,
    /// -t
    exit_after_one_command: bool,
    /// -u
    treat_unset_variables_as_error: bool,
    /// -v
    print_shell_input_lines: bool,
    /// -x
    print_commands_and_arguments: bool,
    /// -B
    perform_brace_expansion: bool,
    /// -C
    disallow_overwriting_regular_files_via_output_redirection: bool,
    /// -E
    shell_functions_inherit_err_trap: bool,
    /// -H
    enable_bang_style_history_substitution: bool,
    /// -P
    do_not_resolve_symlinks_when_changing_dir: bool,
    /// -T
    shell_functions_inherit_debug_and_return_traps: bool,

    //
    // Options set through -o.
    //
    /// 'emacs'
    emacs_mode: bool,
    /// 'history'
    enable_command_history: bool,
    /// 'ignoreeof'
    ignore_eof: bool,
    /// 'interactive-comments'
    allow_comments_in_interactive_commands: bool,
    /// 'pipefail'
    return_first_failure_from_pipeline: bool,
    /// 'posix'
    posix_mode: bool,
    /// 'vi'
    vi_mode: bool,

    //
    // Options set by the shell.
    //
    interactive: bool,
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

impl Default for ShellRuntimeOptions {
    fn default() -> Self {
        Self {
            export_variables_on_modification: false,
            notify_job_termination_immediately: false,
            exit_on_nonzero_command_exit: false,
            disable_filename_globbing: false,
            remember_command_locations: false,
            place_all_assignment_args_in_command_env: false,
            enable_job_control: false,
            do_not_execute_commands: false,
            real_effective_uid_mismatch: false,
            exit_after_one_command: false,
            treat_unset_variables_as_error: false,
            print_shell_input_lines: false,
            print_commands_and_arguments: false,
            perform_brace_expansion: false,
            disallow_overwriting_regular_files_via_output_redirection: false,
            shell_functions_inherit_err_trap: false,
            enable_bang_style_history_substitution: false,
            do_not_resolve_symlinks_when_changing_dir: false,
            shell_functions_inherit_debug_and_return_traps: false,
            emacs_mode: false,
            enable_command_history: false,
            ignore_eof: false,
            allow_comments_in_interactive_commands: false,
            return_first_failure_from_pipeline: false,
            posix_mode: false,
            vi_mode: false,
            interactive: false,
        }
    }
}

#[derive(Debug)]
pub struct ShellCreateOptions {
    pub login: bool,
    pub interactive: bool,
}

type ShellFunction = parser::ast::FunctionDefinition;

enum ProgramOrigin {
    File(PathBuf),
    String,
}

impl Shell {
    pub fn new(options: &ShellCreateOptions) -> Result<Shell> {
        // Seed parameters from environment.
        let mut parameters = HashMap::new();
        for (k, v) in std::env::vars() {
            parameters.insert(
                k,
                ShellVariable {
                    value: v,
                    exported: true,
                    readonly: false,
                },
            );
        }

        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            working_dir: std::env::current_dir()?,
            umask: Default::default(),           // TODO: populate umask
            file_size_limit: Default::default(), // TODO: populate file size limit
            parameters,
            funcs: Default::default(),
            options: ShellRuntimeOptions {
                interactive: options.interactive,
                ..Default::default()
            },
            aliases: Default::default(),
            last_pipeline_exit_status: 0,
        };

        // Load profiles/configuration.
        shell.load_config(options)?;

        Ok(shell)
    }

    fn load_config(&mut self, options: &ShellCreateOptions) -> Result<()> {
        if options.login {
            //
            // TODO: source /etc/profile if it exists
            // TODO: source the first of these that exists and is readable (if any):
            //     * ~/.bash_profile
            //     * ~/.bash_login
            //     * ~/.profile
            // TODO: implement --noprofile to inhibit
            //
            todo!("config for a login shell")
        } else {
            if options.interactive {
                //
                // For non-login interactive shells, load in this order:
                //
                //     /etc/bash.bashrc
                //     ~/.bashrc
                //
                // TODO: implement support for --norc
                //
                self.source_if_exists(std::path::Path::new("/etc/bash.bashrc"))?;
                if let Ok(home_path) = std::env::var("HOME") {
                    self.source_if_exists(
                        std::path::Path::new(&home_path).join(".bashrc").as_path(),
                    )?;
                }
            } else {
                //
                // TODO: look at $BASH_ENV; source its expansion if that file exists
                //
                todo!("config for a non-interactive, non-login shell")
            }
        }

        Ok(())
    }

    fn source_if_exists(&mut self, path: &std::path::Path) -> Result<()> {
        if path.exists() {
            self.source(path, &[])
        } else {
            debug!("skipping non-existent file: {}", path.display());
            Ok(())
        }
    }

    pub fn source(&mut self, path: &std::path::Path, args: &[&str]) -> Result<()> {
        debug!("sourcing: {}", path.display());

        let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut parser = parser::Parser::new(&mut reader);
        let parse_result = parser.parse(false)?;

        // TODO: handle args
        if args.len() > 0 {
            todo!("source with args");
        }

        self.run_parsed_result(&parse_result, &ProgramOrigin::File(path.to_owned()))
    }

    pub fn run_string(&mut self, command: &str) -> Result<()> {
        let mut reader = std::io::BufReader::new(command.as_bytes());
        let mut parser = parser::Parser::new(&mut reader);
        let parse_result = parser.parse(true)?;

        self.run_parsed_result(&parse_result, &ProgramOrigin::String)
    }

    fn run_parsed_result(
        &mut self,
        parse_result: &parser::ParseResult,
        origin: &ProgramOrigin,
    ) -> Result<()> {
        if let Some(prog) = &parse_result.program {
            self.run_program(&prog)?;
        } else {
            let mut error_prefix = "".to_owned();

            if let ProgramOrigin::File(file_path) = origin {
                error_prefix = format!("{}: ", file_path.display());
            }

            if let Some(token_near_error) = &parse_result.token_near_error {
                log::error!(
                    "{}syntax error near token `{}'",
                    error_prefix,
                    token_near_error
                );
            } else {
                log::error!("{}syntax error at end of input", error_prefix);
            }
        }

        Ok(())
    }

    pub fn run_program(&mut self, program: &parser::ast::Program) -> Result<()> {
        program.execute(self)?;

        //
        // Perform any necessary redirections and remove the redirection
        // operators and their operands from the argument list.
        //
        // TODO

        //
        // Execute the command, either as a function, built-in, executable
        // file, or script.
        //
        // TODO

        //
        // Optionally wait for the command to complete and collect its exit
        // status.
        //
        // TODO

        Ok(())
    }

    pub fn run_stdin(&self) -> Result<()> {
        let mut reader = std::io::stdin().lock();
        let mut parser = parser::Parser::new(&mut reader);
        parser.parse(true)?;

        Ok(())
    }

    pub fn compose_prompt(&self) -> Result<String> {
        const DEFAULT_PROMPT: &'static str = "$ ";

        let ps1 = self.parameter_or_default("PS1", DEFAULT_PROMPT);
        let prompt_pieces = parser::prompt::parse_prompt(&ps1)?;

        let formatted_prompt = prompt_pieces
            .iter()
            .map(|p| format_prompt_piece(self, p))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .join("");

        Ok(formatted_prompt)
    }

    pub fn last_result(&self) -> i32 {
        // TODO: implement last_result
        0
    }

    fn parameter_or_default(&self, name: &str, default: &str) -> String {
        self.parameters
            .get(name)
            .map_or_else(|| default.to_owned(), |s| s.value.to_owned())
    }

    pub fn current_option_flags(&self) -> String {
        self.options.get_chars().into_iter().collect()
    }
}
