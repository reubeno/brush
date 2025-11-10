use clap::Parser;
use itertools::Itertools;
use std::collections::VecDeque;

use brush_core::{ErrorKind, builtins, env, error, variables};

use std::io::Read;

/// Parse standard input.
#[derive(Parser)]
pub(crate) struct ReadCommand {
    /// Optionally, name of an array variable to receive read words
    /// of input.
    #[clap(short = 'a', value_name = "VAR_NAME")]
    array_variable: Option<String>,

    /// Optionally, a delimiter to use other than a newline character.
    #[clap(short = 'd')]
    delimiter: Option<String>,

    /// Use readline-like input.
    #[clap(short = 'e')]
    use_readline: bool,

    /// Provide text to use as initial input for readline.
    #[clap(short = 'i', value_name = "STR")]
    initial_text: Option<String>,

    /// Read only the first N characters or until a specified
    /// delimiter is reached, whichever happens first.
    #[clap(short = 'n', value_name = "COUNT")]
    return_after_n_chars: Option<usize>,

    /// Read exactly N characters, ignoring any specified delimiter.
    #[clap(short = 'N', value_name = "COUNT")]
    return_after_n_chars_no_delimiter: Option<usize>,

    /// Prompt to display before reading.
    #[clap(short = 'p')]
    prompt: Option<String>,

    /// Read input in raw mode; no escape sequences.
    #[clap(short = 'r')]
    raw_mode: bool,

    /// Do not echo input.
    #[clap(short = 's')]
    silent: bool,

    /// Specify timeout in seconds; fail if the timeout elapses before
    /// input is completed.
    #[clap(short = 't', value_name = "SECONDS")]
    timeout_in_seconds: Option<usize>,

    /// File descriptor to read from instead of stdin.
    #[clap(short = 'u', name = "FD")]
    fd_num_to_read: Option<u8>,

    /// Optionally, names of variables to receive read input.
    variable_names: Vec<String>,
}

impl builtins::Command for ReadCommand {
    type Error = brush_core::Error;

    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.use_readline {
            return error::unimp("read -e");
        }
        if self.initial_text.is_some() {
            return error::unimp("read -i");
        }
        if self.raw_mode {
            tracing::debug!("read -r is not implemented");
        }
        if self.timeout_in_seconds.is_some() {
            return error::unimp_with_issue("read -t", 227);
        }

        // Find the input stream to use.
        let input_stream = if let Some(fd_num) = self.fd_num_to_read {
            let fd_num = brush_core::ShellFd::from(fd_num);
            context
                .try_fd(fd_num)
                .ok_or_else(|| ErrorKind::BadFileDescriptor(fd_num))?
        } else {
            context
                .try_fd(brush_core::openfiles::OpenFiles::STDIN_FD)
                .unwrap()
        };

        // Retrieve effective value of IFS for splitting.
        let ifs = context.shell.ifs();

        let input_line = self.read_line(input_stream, context.stdout())?;
        let result = if input_line.is_some() {
            brush_core::ExecutionResult::success()
        } else {
            brush_core::ExecutionResult::general_error()
        };

        // If -a was specified, then place the fields as elements into the array.
        if let Some(array_variable) = &self.array_variable {
            let literal_fields = if let Some(input_line) = input_line {
                let fields: VecDeque<_> = split_line_by_ifs(
                    ifs.as_ref(),
                    input_line.as_str(),
                    None, /* max_fields */
                );

                fields.into_iter().map(|f| (None, f)).collect()
            } else {
                vec![]
            };

            context.shell.env.update_or_add(
                array_variable,
                variables::ShellValueLiteral::Array(variables::ArrayLiteral(literal_fields)),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        } else if !self.variable_names.is_empty() {
            let mut fields: VecDeque<_> = if let Some(input_line) = input_line {
                split_line_by_ifs(
                    ifs.as_ref(),
                    input_line.as_str(),
                    /* max_fields */ Some(self.variable_names.len()),
                )
            } else {
                VecDeque::new()
            };

            for (i, name) in self.variable_names.iter().enumerate() {
                if fields.is_empty() {
                    // Ensure the var is empty.
                    context.shell.env.update_or_add(
                        name,
                        variables::ShellValueLiteral::Scalar(String::new()),
                        |_| Ok(()),
                        env::EnvironmentLookup::Anywhere,
                        env::EnvironmentScope::Global,
                    )?;
                    continue;
                }

                let last = i == self.variable_names.len() - 1;
                if !last {
                    let next_field = fields.pop_front().unwrap();
                    context.shell.env.update_or_add(
                        name,
                        variables::ShellValueLiteral::Scalar(next_field),
                        |_| Ok(()),
                        env::EnvironmentLookup::Anywhere,
                        env::EnvironmentScope::Global,
                    )?;
                } else {
                    let remaining_fields = fields.into_iter().join(" ");
                    context.shell.env.update_or_add(
                        name,
                        variables::ShellValueLiteral::Scalar(remaining_fields),
                        |_| Ok(()),
                        env::EnvironmentLookup::Anywhere,
                        env::EnvironmentScope::Global,
                    )?;
                    break;
                }
            }
        } else {
            let input_line = input_line.unwrap_or_default();

            // If no variable names were specified, then place everything into the
            // REPLY variable.
            context.shell.env.update_or_add(
                "REPLY",
                variables::ShellValueLiteral::Scalar(input_line),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        }

        Ok(result)
    }
}

enum ReadTermination {
    Delimiter,
    EndOfInput,
    CtrlC,
    Limit,
}

impl ReadCommand {
    fn read_line(
        &self,
        mut input_file: brush_core::openfiles::OpenFile,
        mut output_file: impl std::io::Write,
    ) -> Result<Option<String>, brush_core::Error> {
        let _term_mode = self.setup_terminal_settings(&input_file)?;

        let delimiter = if self.return_after_n_chars_no_delimiter.is_some() {
            None
        } else if let Some(delimiter_str) = &self.delimiter {
            // If the delimiter string is empty, then the docs indicate we need to use
            // the NUL character as the actual delimiter.
            if delimiter_str.is_empty() {
                Some('\0')
            } else {
                // In other cases, use the first character in the string as the delimiter.
                delimiter_str.chars().next()
            }
        } else {
            Some('\n')
        };

        let char_limit = self
            .return_after_n_chars_no_delimiter
            .or(self.return_after_n_chars);

        if let Some(prompt) = &self.prompt {
            write!(output_file, "{prompt}")?;
            output_file.flush()?;
        }

        let mut line = String::new();
        let mut buffer = [0; 1]; // 1-byte buffer

        let reason = loop {
            // TODO: Figure out how to restore terminal settings on error?
            let n = input_file.read(&mut buffer)?;
            if n == 0 {
                break ReadTermination::EndOfInput; // EOF reached.
            }

            let ch = buffer[0] as char;

            // Check for Ctrl+C.
            if ch == '\x03' {
                break ReadTermination::CtrlC;
            } else if ch == '\x04' {
                // Ctrl+D is EOF.
                break ReadTermination::EndOfInput;
            }

            // Check for a delimiter that indicates end-of-input.
            if let Some(delimiter) = delimiter {
                if ch == delimiter {
                    break ReadTermination::Delimiter;
                }
            }

            // Ignore other control characters without including them in the input.
            if ch.is_ascii_control() && !ch.is_ascii_whitespace() {
                continue;
            }

            line.push(ch);

            // Check to see if we've hit a character limit.
            if let Some(char_limit) = char_limit {
                if line.len() >= char_limit {
                    break ReadTermination::Limit;
                }
            }
        };

        match reason {
            ReadTermination::EndOfInput => {
                if line.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(line))
                }
            }
            ReadTermination::CtrlC => {
                // Discard the input and return.
                Ok(None)
            }
            ReadTermination::Delimiter | ReadTermination::Limit => Ok(Some(line)),
        }
    }

    fn setup_terminal_settings(
        &self,
        file: &brush_core::openfiles::OpenFile,
    ) -> Result<Option<brush_core::terminal::AutoModeGuard>, brush_core::Error> {
        let mode = brush_core::terminal::AutoModeGuard::new(file.to_owned()).ok();
        if let Some(mode) = &mode {
            let config = brush_core::terminal::Settings::builder()
                .line_input(false)
                .interrupt_signals(false)
                .echo_input(!self.silent)
                .build();

            mode.apply_settings(&config)?;
        }

        Ok(mode)
    }
}

fn split_line_by_ifs(ifs: &str, line: &str, max_fields: Option<usize>) -> VecDeque<String> {
    // Separate out the chars to split by.
    let ifs_chars = ifs.chars().collect::<Vec<_>>();

    // Compute which IFS characters are one of the default 3 whitespace chars.
    let ifs_space_chars = ifs_chars
        .iter()
        .copied()
        .filter(|c| *c == ' ' || *c == '\t' || *c == '\n')
        .collect::<Vec<_>>();

    // First, trim from the prefix and suffix of the string any matching chars that are
    // *both* whitespace and present in the IFS string.
    let trimmed_line = line.trim_matches(ifs_space_chars.as_slice());
    if trimmed_line.is_empty() {
        return VecDeque::new();
    }

    let max_fields = max_fields.unwrap_or(usize::MAX);

    // Now, iterate through the string, manually splitting it by the shell's rules for
    // honoring IFS. We implement this by hand because we need to ensure that any
    // IFS character is a valid delimiter, *but* consecutive adjacent IFS characters
    // are considered a single delimiter if and only if they are one of the 3 default
    // whitespace characters (i.e., ' ', '\t', or '\n').
    let mut fields = VecDeque::new();
    let mut current_field = String::new();
    let mut skipping_ifs_whitespace = false;

    for c in trimmed_line.chars() {
        if skipping_ifs_whitespace {
            if ifs_space_chars.contains(&c) {
                continue;
            }

            skipping_ifs_whitespace = false;
        }

        if fields.len() + 1 < max_fields && ifs_chars.contains(&c) {
            fields.push_back(current_field);
            current_field = String::new();
            skipping_ifs_whitespace = ifs_space_chars.contains(&c);
        } else {
            current_field.push(c);
        }
    }

    fields.push_back(current_field);

    fields
}

#[cfg(test)]
mod tests {
    use itertools::assert_equal;

    use super::*;

    #[test]
    fn test_split_line_by_ifs() {
        let result = split_line_by_ifs(",", "a,b,c", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_leading_or_trailing_space() {
        // Test with leading or trailing space.
        let result = split_line_by_ifs(" ", "  a b c ", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_extra_interior_space() {
        // Test with leading or trailing space.
        let result = split_line_by_ifs(" ", "a  b c", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_leading_non_space_delimiter() {
        let result = split_line_by_ifs(",", ",a,b,c", None);
        assert_equal(result, VecDeque::from(vec!["", "a", "b", "c"]));
    }

    #[test]
    fn test_split_line_by_ifs_trailing_non_space_delimiter() {
        let result = split_line_by_ifs(",", "a,b,c,", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c", ""]));
    }
}
