use anyhow::Result;
use log::debug;
use std::collections::HashMap;

use crate::{context::ExecutionContext, interp::Execute};

type Editor = rustyline::Editor<(), rustyline::history::MemHistory>;

pub struct Shell {
    editor: Editor,
    context: ExecutionContext,
}

pub struct ShellOptions {
    pub login: bool,
    pub interactive: bool,
}

impl Shell {
    pub fn new(options: &ShellOptions) -> Result<Shell> {
        // Seed parameters from environment.
        let mut parameters = HashMap::new();
        for (k, v) in std::env::vars() {
            parameters.insert(k, v);
        }

        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            editor: Self::new_editor()?,
            context: ExecutionContext {
                working_dir: std::env::current_dir()?,
                umask: Default::default(),           // TODO: populate umask
                file_size_limit: Default::default(), // TODO: populate file size limit
                parameters,
                funcs: Default::default(),
                aliases: Default::default(),
                last_pipeline_exit_status: 0,
            },
        };

        // Load profiles/configuration.
        shell.load_config(options)?;

        Ok(shell)
    }

    fn load_config(&mut self, options: &ShellOptions) -> Result<()> {
        if options.interactive {
            if options.login {
                //
                // TODO: Do something appropriate for login shells.
                //
            } else {
                //
                // For non-login interactive shells, load in this order:
                //
                //     /etc/bash.bashrc
                //     ~/.bashrc
                //
                self.source_if_exists(std::path::Path::new("/etc/bash.bashrc"))?;
                if let Ok(home_path) = std::env::var("HOME") {
                    self.source_if_exists(
                        std::path::Path::new(&home_path).join(".bashrc").as_path(),
                    )?;
                }
            }
        } else {
            //
            // TODO: Do something appropriate for non-interactive shells.
            //
        }

        Ok(())
    }

    fn source_if_exists(&mut self, path: &std::path::Path) -> Result<()> {
        if path.exists() {
            self.source(path)
        } else {
            debug!("skipping non-existent file: {}", path.display());
            Ok(())
        }
    }

    fn source(&mut self, path: &std::path::Path) -> Result<()> {
        debug!("sourcing: {}", path.display());

        let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut parser = parser::Parser::new(&mut reader);
        let parse_result = parser.parse(false)?;

        self.run_parsed_result(&parse_result)
    }

    fn new_editor() -> Result<Editor> {
        let config = rustyline::config::Builder::new()
            .max_history_size(1000)?
            .history_ignore_dups(true)?
            .auto_add_history(true)
            .build();

        // TODO: Create an editor with a helper object so we can do completion.
        let editor = rustyline::Editor::<(), _>::with_history(
            config,
            rustyline::history::MemHistory::with_config(config),
        )?;

        Ok(editor)
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
        program.execute(&mut self.context)?;

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

    pub fn run_interactively(&mut self) -> Result<()> {
        loop {
            if self.run_interactively_once()? {
                break;
            }
        }

        Ok(())
    }

    pub fn run_stdin(&self) -> Result<()> {
        let mut reader = std::io::stdin().lock();
        let mut parser = parser::Parser::new(&mut reader);
        parser.parse(true)?;

        Ok(())
    }

    fn run_interactively_once(&mut self) -> Result<bool> {
        let prompt = self.compose_prompt()?;

        match self.editor.readline(&prompt) {
            Ok(read_result) => {
                self.run_string(&read_result)?;
                Ok(false)
            }
            Err(rustyline::error::ReadlineError::Eof) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }

    fn compose_prompt(&self) -> Result<String> {
        const DEFAULT_PROMPT: &'static str = "$ ";

        let ps1 = self.parameter_or_default("PS1", DEFAULT_PROMPT);
        let prompt_pieces = parse_prompt(&ps1)?;

        let formatted_prompt = prompt_pieces
            .iter()
            .map(format_prompt_piece)
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
        self.context
            .parameters
            .get(name)
            .map_or_else(|| default.to_owned(), |s| s.to_owned())
    }
}

enum ShellPromptPiece {
    Literal(String),
}

fn parse_prompt(s: &str) -> Result<Vec<ShellPromptPiece>> {
    //
    // TODO: implement parsing of prompt specifier
    //

    Ok(vec![ShellPromptPiece::Literal(s.to_owned())])
}

fn format_prompt_piece(piece: &ShellPromptPiece) -> Result<String> {
    let formatted = match piece {
        ShellPromptPiece::Literal(l) => l.to_owned(),
    };

    Ok(formatted)
}
