use std::{borrow::Cow, path::PathBuf};

use anyhow::Result;
use rustyline::validate::ValidationResult;

type Editor = rustyline::Editor<EditorHelper, rustyline::history::FileHistory>;

pub struct Options {
    pub shell: shell::CreateOptions,
    pub disable_bracketed_paste: bool,
}

pub struct InteractiveShell {
    editor: Editor,
    history_file_path: Option<PathBuf>,
}

enum InteractiveExecutionResult {
    Executed(shell::ExecutionResult),
    Failed(shell::Error),
    Eof,
}

impl InteractiveShell {
    pub async fn new(options: &Options) -> Result<InteractiveShell> {
        // Set up shell first. Its initialization may influence how the
        // editor needs to operate.
        let shell = shell::Shell::new(&options.shell).await?;
        let history_file_path = shell.get_history_file_path();

        let mut editor = Self::new_editor(options, shell)?;
        if let Some(history_file_path) = &history_file_path {
            if !history_file_path.exists() {
                std::fs::File::create(history_file_path)?;
            }

            editor.load_history(history_file_path)?;
        }

        Ok(InteractiveShell {
            editor,
            history_file_path,
        })
    }

    pub fn shell(&self) -> &shell::Shell {
        &self.editor.helper().unwrap().shell
    }

    pub fn shell_mut(&mut self) -> &mut shell::Shell {
        &mut self.editor.helper_mut().unwrap().shell
    }

    fn new_editor(options: &Options, shell: shell::Shell) -> Result<Editor> {
        let config = rustyline::config::Builder::new()
            .max_history_size(1000)?
            .history_ignore_dups(true)?
            .auto_add_history(true)
            .bell_style(rustyline::config::BellStyle::None)
            .completion_type(rustyline::config::CompletionType::List)
            .bracketed_paste(!options.disable_bracketed_paste)
            .build();

        let mut editor = rustyline::Editor::with_config(config)?;
        editor.set_helper(Some(EditorHelper::new(shell)));

        Ok(editor)
    }

    pub async fn run_interactively(&mut self) -> Result<()> {
        loop {
            let result = self.run_interactively_once().await?;
            match result {
                InteractiveExecutionResult::Executed(shell::ExecutionResult {
                    exit_shell,
                    return_from_function_or_script,
                    ..
                }) => {
                    if exit_shell {
                        break;
                    }

                    if return_from_function_or_script {
                        log::error!("return from non-function/script");
                    }
                }
                InteractiveExecutionResult::Failed(e) => {
                    // Report the error, but continue to execute.
                    log::error!("error: {:#}", e);
                }
                InteractiveExecutionResult::Eof => {
                    break;
                }
            }
        }

        if self.shell().options.interactive {
            // TODO: Write this to the correct output stream.
            eprintln!("exit");
        }

        if let Some(history_file_path) = &self.history_file_path {
            // TODO: Decide append or not based on configuration.
            self.editor.append_history(history_file_path)?;
        }

        Ok(())
    }

    async fn run_interactively_once(&mut self) -> Result<InteractiveExecutionResult> {
        // If there's a variable called PROMPT_COMMAND, then run it first.
        if let Some(prompt_cmd) = self.shell().env.get("PROMPT_COMMAND") {
            let prompt_cmd: String = prompt_cmd.value().into();
            self.shell_mut()
                .run_string(prompt_cmd.as_str(), false)
                .await?;
        }

        // Now that we've done that, compose the prompt.
        let prompt = self.shell_mut().compose_prompt().await?;

        match self.editor.readline(&prompt) {
            Ok(read_result) => match self.shell_mut().run_string(&read_result, false).await {
                Ok(result) => Ok(InteractiveExecutionResult::Executed(result)),
                Err(e) => Ok(InteractiveExecutionResult::Failed(e)),
            },
            Err(rustyline::error::ReadlineError::Eof) => Ok(InteractiveExecutionResult::Eof),
            Err(rustyline::error::ReadlineError::Interrupted) => {
                self.shell_mut().last_exit_status = 130;
                Ok(InteractiveExecutionResult::Executed(
                    shell::ExecutionResult::new(130),
                ))
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(rustyline::Helper, rustyline::Hinter)]
pub(crate) struct EditorHelper {
    pub shell: shell::Shell,

    #[rustyline(Hinter)]
    hinter: rustyline::hint::HistoryHinter,
}

impl EditorHelper {
    pub(crate) fn new(shell: shell::Shell) -> Self {
        // let completer = InteractiveShellCompleter::new(shell);
        let hinter = rustyline::hint::HistoryHinter::new();
        Self { shell, hinter }
    }

    fn get_completion_candidate_display_str(s: &str) -> String {
        let s_without_trailing_space = s.trim_end();
        let s_without_final_slash = s_without_trailing_space.strip_suffix('/').unwrap_or(s);

        let s = if let Some(slash_index) = s_without_final_slash.rfind('/') {
            &s[slash_index + 1..]
        } else {
            s
        };

        s.to_owned()
    }
}

impl rustyline::highlight::Highlighter for EditorHelper {
    // Display hints with low intensity
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned("\x1b[2m".to_owned() + hint + "\x1b[m")
    }

    // Color names that seem to be dirs.
    fn highlight_candidate<'c>(
        &self,
        candidate: &'c str, // FIXME should be Completer::Candidate
        _completion: rustyline::CompletionType,
    ) -> Cow<'c, str> {
        if let Some(candidate_without_suffix) = candidate.strip_suffix('/') {
            Cow::Owned("\x1b[1;34m".to_owned() + candidate_without_suffix + "\x1b[0m/")
        } else {
            Cow::Borrowed(candidate)
        }
    }
}

impl rustyline::completion::Completer for EditorHelper {
    type Candidate = rustyline::completion::Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        // Intentionally ignore any errors that arise.
        let completions = self.shell.get_completions(line, pos).unwrap_or_default();
        let candidates = completions
            .candidates
            .into_iter()
            .map(|c| rustyline::completion::Pair {
                display: Self::get_completion_candidate_display_str(c.as_str()),
                replacement: c,
            })
            .collect();

        Ok((completions.start, candidates))
    }
}

impl rustyline::validate::Validator for EditorHelper {
    fn validate(
        &self,
        ctx: &mut rustyline::validate::ValidationContext,
    ) -> rustyline::Result<rustyline::validate::ValidationResult> {
        let line = ctx.input();

        let parse_result = self.shell.parse_string(line);

        let validation_result = match parse_result {
            Err(parser::ParseError::Tokenizing { inner, position: _ }) if inner.is_incomplete() => {
                ValidationResult::Incomplete
            }
            Err(parser::ParseError::ParsingAtEndOfInput) => ValidationResult::Incomplete,
            _ => ValidationResult::Valid(None),
        };

        Ok(validation_result)
    }
}
