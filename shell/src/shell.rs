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
    // TODO: Add other options.
}

impl Default for ShellRuntimeOptions {
    fn default() -> Self {
        Self {}
    }
}

#[derive(Debug)]
pub struct ShellOptions {
    pub login: bool,
    pub interactive: bool,
}

type ShellFunction = parser::ast::FunctionDefinition;

impl Shell {
    pub fn new(options: &ShellOptions) -> Result<Shell> {
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
            options: Default::default(),
            aliases: Default::default(),
            last_pipeline_exit_status: 0,
        };

        // Load profiles/configuration.
        shell.load_config(options)?;

        Ok(shell)
    }

    fn load_config(&mut self, options: &ShellOptions) -> Result<()> {
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

        self.run_parsed_result(&parse_result)
    }

    pub fn run_string(&mut self, command: &str) -> Result<()> {
        let mut reader = std::io::BufReader::new(command.as_bytes());
        let mut parser = parser::Parser::new(&mut reader);
        let parse_result = parser.parse(true)?;

        self.run_parsed_result(&parse_result)
    }

    fn run_parsed_result(&mut self, parse_result: &parser::ParseResult) -> Result<()> {
        if let Some(prog) = &parse_result.program {
            self.run_program(&prog)?;
        } else {
            if let Some(token_near_error) = &parse_result.token_near_error {
                log::error!("syntax error near token `{}'", token_near_error);
            } else {
                log::error!("syntax error at end of input");
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
}
