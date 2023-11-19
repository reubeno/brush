use anyhow::Result;

type Editor = rustyline::Editor<(), rustyline::history::MemHistory>;

pub struct InteractiveShell {
    pub shell: shell::Shell,

    editor: Editor,
}

impl InteractiveShell {
    pub fn new(login: bool) -> Result<InteractiveShell> {
        Ok(InteractiveShell {
            editor: Self::new_editor()?,
            shell: shell::Shell::new(&shell::ShellOptions {
                login,
                interactive: true,
            })?,
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
            if self.run_interactively_once()? {
                break;
            }
        }

        Ok(())
    }

    fn run_interactively_once(&mut self) -> Result<bool> {
        let prompt = self.shell.compose_prompt()?;

        match self.editor.readline(&prompt) {
            Ok(read_result) => {
                self.shell.run_string(&read_result)?;
                Ok(false)
            }
            Err(rustyline::error::ReadlineError::Eof) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }
}
