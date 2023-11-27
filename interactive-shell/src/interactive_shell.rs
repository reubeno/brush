use anyhow::Result;

type Editor = rustyline::Editor<(), rustyline::history::MemHistory>;

pub struct InteractiveShell {
    pub shell: shell::Shell,

    editor: Editor,
}

enum InteractiveExecutionResult {
    Executed(shell::ExecutionResult),
    Eof,
}

impl InteractiveShell {
    pub fn new(options: &shell::ShellCreateOptions) -> Result<InteractiveShell> {
        Ok(InteractiveShell {
            editor: Self::new_editor()?,
            shell: shell::Shell::new(options)?,
        })
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

    pub fn run_interactively(&mut self) -> Result<()> {
        loop {
            let result = self.run_interactively_once()?;
            match result {
                InteractiveExecutionResult::Executed(shell::ExecutionResult {
                    exit_code: _,
                    exit_shell,
                }) => {
                    if exit_shell {
                        break;
                    }
                }
                InteractiveExecutionResult::Eof => break,
            }
        }

        Ok(())
    }

    fn run_interactively_once(&mut self) -> Result<InteractiveExecutionResult> {
        let prompt = self.shell.compose_prompt()?;

        match self.editor.readline(&prompt) {
            Ok(read_result) => {
                let result = self.shell.run_string(&read_result)?;
                Ok(InteractiveExecutionResult::Executed(result))
            }
            Err(rustyline::error::ReadlineError::Eof) => Ok(InteractiveExecutionResult::Eof),
            Err(e) => Err(e.into()),
        }
    }
}
