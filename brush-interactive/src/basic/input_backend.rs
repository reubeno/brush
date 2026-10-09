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
pub struct BasicInputBackend {
    /// Whether, like readline's `show-all-if-ambiguous`, to list several candidates as soon as
    /// they're completed to their common prefix, rather than on the next completion.
    show_all_if_ambiguous: bool,
}

impl InputBackend for BasicInputBackend {
    fn read_line(
        &mut self,
        shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
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
    /// Returns a basic input backend with the given options.
    ///
    /// # Arguments
    ///
    /// * `options` - The options for the shell's user interface.
    pub const fn new(options: &crate::UIOptions) -> Self {
        Self {
            show_all_if_ambiguous: options.show_all_if_ambiguous,
        }
    }

    fn read_line_via<R: super::LineReader, SE: brush_core::ShellExtensions>(
        &self,
        shell_ref: &crate::ShellRef<SE>,
        reader: &R,
        prompt: &InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        let mut prompt_to_use = self.should_display_prompt().then_some(&prompt);
        let show_all_if_ambiguous = self.show_all_if_ambiguous;
        let mut result = String::new();

        loop {
            match reader.read_line(prompt_to_use.map(|p| p.prompt.as_str()), |line, cursor| {
                let mut shell = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(shell_ref.lock())
                });

                Self::generate_completions(&mut shell, line, cursor, show_all_if_ambiguous)
            })? {
                ReadResult::Input(s) => {
                    result.push_str(s.as_str());

                    if !crate::completeness::needs_more_input(shell_ref, result.as_str()) {
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

    fn generate_completions(
        shell: &mut Shell<impl brush_core::ShellExtensions>,
        line: &str,
        cursor: usize,
        show_all_if_ambiguous: bool,
    ) -> Result<crate::completion::Offers, ShellError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(Self::generate_completions_async(
                shell,
                line,
                cursor,
                show_all_if_ambiguous,
            ))
        })
    }

    async fn generate_completions_async(
        shell: &mut Shell<impl brush_core::ShellExtensions>,
        line: &str,
        cursor: usize,
        show_all_if_ambiguous: bool,
    ) -> Result<crate::completion::Offers, ShellError> {
        Ok(completion::complete_async(shell, line, cursor, show_all_if_ambiguous).await)
    }
}
