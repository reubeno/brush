use std::io::IsTerminal;

use brush_core::Shell;

use crate::{
    InputBackend, ShellError, completion,
    input_backend::{InteractivePrompt, ReadResult},
};

use super::{non_term_line_reader, term_line_reader};

/// Represents a basic shell input backend capable of interactive usage, with primitive support
/// for completion and test-focused automation via pexpect and similar technologies.
#[derive(Default)]
pub struct BasicInputBackend;

impl InputBackend for BasicInputBackend {
    fn read_line(
        &mut self,
        shell: &crate::ShellRef,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        if std::io::stdin().is_terminal() {
            self.read_line_via(shell, &term_line_reader::TermLineReader::new()?, &prompt)
        } else {
            self.read_line_via(shell, &non_term_line_reader::NonTermLineReader, &prompt)
        }
    }
}

impl BasicInputBackend {
    fn read_line_via<R: super::LineReader>(
        &self,
        shell_ref: &crate::ShellRef,
        reader: &R,
        prompt: &InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        let mut prompt_to_use = self.should_display_prompt().then_some(&prompt);
        let mut result = String::new();

        loop {
            match reader.read_line(prompt_to_use.map(|p| p.prompt.as_str()), |line, cursor| {
                let mut shell = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(shell_ref.lock())
                });

                Self::generate_completions(&mut shell, line, cursor)
            })? {
                ReadResult::Input(s) => {
                    result.push_str(s.as_str());

                    let shell = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(shell_ref.lock())
                    });

                    if Self::is_valid_input(&shell, result.as_str()) {
                        break;
                    }

                    prompt_to_use = None;
                }
                ReadResult::BoundCommand(s) => {
                    result.push_str(s.as_str());
                    break;
                }
                ReadResult::Eof => {
                    if result.is_empty() {
                        return Ok(ReadResult::Eof);
                    }
                    break;
                }
                ReadResult::Interrupted => return Ok(ReadResult::Interrupted),
            }
        }

        Ok(ReadResult::Input(result))
    }

    #[expect(clippy::unused_self)]
    fn should_display_prompt(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn is_valid_input(shell: &impl ShellRuntime, input: &str) -> bool {
        match shell.parse_string(input.to_owned()) {
            Err(brush_parser::ParseError::Tokenizing { inner, position: _ })
                if inner.is_incomplete() =>
            {
                false
            }
            Err(brush_parser::ParseError::ParsingAtEndOfInput) => false,
            _ => true,
        }
    }

    fn generate_completions(
        shell: &mut Shell,
        line: &str,
        cursor: usize,
    ) -> Result<brush_core::completion::Completions, ShellError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(Self::generate_completions_async(shell, line, cursor))
        })
    }

    async fn generate_completions_async(
        shell: &mut Shell,
        line: &str,
        cursor: usize,
    ) -> Result<brush_core::completion::Completions, ShellError> {
        Ok(completion::complete_async(shell, line, cursor).await)
    }
}
