use std::io::{BufRead, IsTerminal, Write};

use crate::{
    InputBackend, ShellError,
    input_backend::{InteractivePrompt, ReadResult},
};

/// Represents a minimal shell input backend, capable of taking commands from standard input.
#[derive(Default)]
pub struct MinimalInputBackend;

impl InputBackend for MinimalInputBackend {
    fn read_line(
        &mut self,
        shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        let stdin = std::io::stdin();
        let prompt = stdin.is_terminal().then_some(&prompt);

        Self::read_program_from(shell_ref, prompt, &mut stdin.lock(), &mut std::io::stderr())
    }
}

impl MinimalInputBackend {
    /// Reads lines from the given reader until they form a complete shell program.
    /// Compound commands, here documents, and backslash continuations all span lines,
    /// so a single line is often not a program on its own.
    ///
    /// # Arguments
    ///
    /// * `shell_ref` - The shell whose parsing options decide when input is complete.
    /// * `prompt` - The prompt to display, or `None` to display nothing.
    /// * `reader` - The source to read lines from.
    /// * `writer` - Where prompts are written.
    fn read_program_from<R: BufRead, W: Write>(
        shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: Option<&InteractivePrompt>,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<ReadResult, ShellError> {
        let mut result = String::new();

        loop {
            if let Some(prompt) = prompt {
                let to_display = if result.is_empty() {
                    &prompt.prompt
                } else {
                    &prompt.continuation_prompt
                };

                write!(writer, "{to_display}")?;
                writer.flush()?;
            }

            // Nothing more is coming; run what we have and let the parser complain if
            // it's still incomplete.
            let Some(line) = Self::read_input_line(reader)? else {
                break;
            };

            result.push_str(line.as_str());

            if !crate::completeness::needs_more_input(shell_ref, result.as_str()) {
                break;
            }
        }

        if result.is_empty() {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(result))
        }
    }

    /// Reads a single line, returning `None` at end of input.
    fn read_input_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, ShellError> {
        let mut input = String::new();
        let bytes_read = reader
            .read_line(&mut input)
            .map_err(ShellError::InputError)?;

        Ok((bytes_read > 0).then_some(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Cursor, sync::Arc};

    async fn test_shell_ref() -> crate::ShellRef {
        Arc::new(tokio::sync::Mutex::new(
            brush_core::Shell::builder().build().await.unwrap(),
        ))
    }

    fn test_prompt() -> InteractivePrompt {
        InteractivePrompt {
            prompt: String::from("$ "),
            alt_side_prompt: String::new(),
            continuation_prompt: String::from("> "),
        }
    }

    /// Reads one program from `input`, returning what the backend accumulated.
    async fn read_program(input: &str) -> ReadResult {
        let shell_ref = test_shell_ref().await;
        let mut reader = Cursor::new(input.as_bytes());

        MinimalInputBackend::read_program_from(&shell_ref, None, &mut reader, &mut Vec::new())
            .unwrap()
    }

    async fn assert_reads(input: &str, expected: &str) {
        let actual = match read_program(input).await {
            ReadResult::Input(s) => format!("Input({s:?})"),
            ReadResult::BoundCommand(s) => format!("BoundCommand({s:?})"),
            ReadResult::Eof => String::from("Eof"),
            ReadResult::Interrupted => String::from("Interrupted"),
        };

        assert_eq!(
            actual,
            format!("Input({expected:?})"),
            "for input {input:?}"
        );
    }

    #[tokio::test]
    async fn stops_at_a_complete_program() {
        assert_reads("echo one\necho two\n", "echo one\n").await;
    }

    #[tokio::test]
    async fn accumulates_until_complete() {
        assert_reads("true &&\necho yes\n", "true &&\necho yes\n").await;
        assert_reads(
            "if true\nthen\necho yes\nfi\necho after\n",
            "if true\nthen\necho yes\nfi\n",
        )
        .await;
        assert_reads("cat <<EOF\nbody\nEOF\n", "cat <<EOF\nbody\nEOF\n").await;
        assert_reads("echo a\\\nb\n", "echo a\\\nb\n").await;
    }

    #[tokio::test]
    async fn returns_incomplete_input_at_end_of_input() {
        // Truncated mid-construct: hand it over anyway so the parser reports the error.
        assert_reads("if true\nthen\n", "if true\nthen\n").await;
    }

    #[tokio::test]
    async fn reports_end_of_input_when_there_is_nothing_to_read() {
        assert!(matches!(read_program("").await, ReadResult::Eof));
    }

    #[tokio::test]
    async fn displays_a_continuation_prompt_for_each_extra_line() {
        let shell_ref = test_shell_ref().await;
        let prompt = test_prompt();
        let mut prompts = Vec::new();

        MinimalInputBackend::read_program_from(
            &shell_ref,
            Some(&prompt),
            &mut Cursor::new(b"if true\nthen\necho hi\nfi\n".as_slice()),
            &mut prompts,
        )
        .unwrap();

        assert_eq!(String::from_utf8(prompts).unwrap(), "$ > > > ");
    }

    #[tokio::test]
    async fn displays_no_prompts_without_one_to_display() {
        let shell_ref = test_shell_ref().await;
        let mut prompts = Vec::new();

        MinimalInputBackend::read_program_from(
            &shell_ref,
            None,
            &mut Cursor::new(b"if true\nthen\necho hi\nfi\n".as_slice()),
            &mut prompts,
        )
        .unwrap();

        assert!(prompts.is_empty());
    }
}
