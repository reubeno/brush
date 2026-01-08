use clap::Parser;
use itertools::Itertools;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use brush_core::{ErrorKind, builtins, env, error, variables};

use std::io::{Read, Write};

/// Exit code returned when `read` times out.
/// This is 128 + SIGALRM (14) = 142, matching bash behavior.
const TIMEOUT_EXIT_CODE: u8 = 142;

/// ASCII control character for Ctrl+C (ETX - End of Text).
const CTRL_C: char = '\x03';
/// ASCII control character for Ctrl+D (EOT - End of Transmission).
const CTRL_D: char = '\x04';
/// Backslash character used for escape processing.
const BACKSLASH: char = '\\';
/// Default line delimiter (newline).
const DEFAULT_DELIMITER: char = '\n';
/// NUL character used as delimiter when `-d ''` is specified.
const NUL_DELIMITER: char = '\0';

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
    #[clap(short = 't', value_name = "SECONDS", allow_hyphen_values = true)]
    timeout_in_seconds: Option<f64>,

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

        // Validate timeout value if provided.
        // TODO(read): Bash uses $TMOUT as a default timeout for `read` when -t is not specified.
        // This is separate from TMOUT's role in interactive shell idle timeout.
        if let Some(timeout) = self.timeout_in_seconds {
            if timeout < 0.0 {
                writeln!(
                    context.stderr(),
                    "{}: -t: {timeout}: invalid timeout specification",
                    context.command_name
                )?;
                return Ok(brush_core::ExecutionResult::general_error());
            }
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

        // Convert timeout to Duration.
        let timeout = self.timeout_in_seconds.map(Duration::from_secs_f64);

        // Perform the read operation (potentially with timeout).
        let read_result = self.read_line(input_stream, context.stdout(), timeout)?;

        // Determine whether to skip IFS splitting (for -N option).
        let skip_ifs_splitting = self.return_after_n_chars_no_delimiter.is_some();

        // Extract the input line and determine exit code based on result.
        let (input_line, result) = match &read_result {
            ReadResult::Line(line) => (Some(line.clone()), brush_core::ExecutionResult::success()),
            ReadResult::Eof(Some(line)) => (
                Some(line.clone()),
                brush_core::ExecutionResult::general_error(),
            ),
            ReadResult::Eof(None) | ReadResult::Interrupted | ReadResult::InputNotReady => {
                (None, brush_core::ExecutionResult::general_error())
            }
            ReadResult::TimedOut(partial) => (
                partial.clone(),
                brush_core::ExecutionResult::new(TIMEOUT_EXIT_CODE),
            ),
            ReadResult::InputReady => (None, brush_core::ExecutionResult::success()),
        };

        // Assign input to variables based on options.
        if let Some(array_variable) = &self.array_variable {
            let literal_fields =
                build_array_fields(input_line.as_deref(), &ifs, skip_ifs_splitting);
            context.shell.env_mut().update_or_add(
                array_variable,
                variables::ShellValueLiteral::Array(variables::ArrayLiteral(literal_fields)),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        } else if !self.variable_names.is_empty() {
            let mut fields = build_variable_fields(
                input_line.as_deref(),
                &ifs,
                skip_ifs_splitting,
                self.variable_names.len(),
            );

            for (i, name) in self.variable_names.iter().enumerate() {
                if fields.is_empty() {
                    context.shell.env_mut().update_or_add(
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
                    context.shell.env_mut().update_or_add(
                        name,
                        variables::ShellValueLiteral::Scalar(next_field),
                        |_| Ok(()),
                        env::EnvironmentLookup::Anywhere,
                        env::EnvironmentScope::Global,
                    )?;
                } else {
                    let remaining_fields = fields.into_iter().join(" ");
                    context.shell.env_mut().update_or_add(
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
            context.shell.env_mut().update_or_add(
                "REPLY",
                variables::ShellValueLiteral::Scalar(input_line.unwrap_or_default()),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        }

        Ok(result)
    }
}

/// Builds array field values from input, optionally splitting by IFS.
fn build_array_fields(
    input_line: Option<&str>,
    ifs: &str,
    skip_ifs_splitting: bool,
) -> Vec<(Option<String>, String)> {
    match input_line {
        Some(line) if skip_ifs_splitting => {
            // With -N, don't split - put entire input as single element.
            vec![(None, line.to_string())]
        }
        Some(line) => {
            let fields: VecDeque<_> = split_line_by_ifs(ifs, line, None /* max_fields */);
            fields.into_iter().map(|f| (None, f)).collect()
        }
        None => vec![],
    }
}

/// Builds field values from input for assignment to named variables.
fn build_variable_fields(
    input_line: Option<&str>,
    ifs: &str,
    skip_ifs_splitting: bool,
    num_variables: usize,
) -> VecDeque<String> {
    match input_line {
        Some(line) if skip_ifs_splitting => {
            // With -N, don't split - put entire input in first variable.
            let mut fields = VecDeque::new();
            fields.push_back(line.to_string());
            fields
        }
        Some(line) => split_line_by_ifs(ifs, line, Some(num_variables)),
        None => VecDeque::new(),
    }
}

/// Result of a `read` operation.
///
/// This enum clearly represents all possible outcomes of `read_line()`,
/// making the contract with callers explicit.
enum ReadResult {
    /// Successfully read a complete line (delimiter or char limit reached).
    Line(String),
    /// Reached end of input. Contains any partial content read before EOF.
    Eof(Option<String>),
    /// Input was interrupted (e.g., Ctrl+C). No content is returned.
    Interrupted,
    /// The operation timed out. Contains any partial content read before timeout.
    TimedOut(Option<String>),
    /// For `-t 0`: input is immediately available (exit 0).
    InputReady,
    /// For `-t 0`: no input immediately available (exit 1).
    InputNotReady,
}

/// Helper struct that encapsulates the state for reading input character by character.
///
/// This separates the concerns of character-level I/O with timeout handling from the
/// higher-level logic of line building and escape processing.
struct InputReader<'a> {
    /// The input source.
    input: brush_core::openfiles::OpenFile,
    /// Optional deadline for timeout.
    deadline: Option<Instant>,
    /// Single-byte read buffer.
    buffer: [u8; 1],
    /// Terminal mode guard (kept alive for duration of read).
    _term_mode: Option<brush_core::terminal::AutoModeGuard>,
    /// Output for prompt display.
    output: &'a mut dyn std::io::Write,
}

/// Events that can occur when reading input.
enum InputEvent {
    /// A regular character was read.
    Char(char),
    /// End of file was reached.
    Eof,
    /// The read operation timed out.
    Timeout,
    /// Ctrl+C was pressed.
    CtrlC,
    /// Ctrl+D was pressed.
    CtrlD,
}

impl<'a> InputReader<'a> {
    /// Creates a new input reader with optional timeout.
    fn new(
        input: brush_core::openfiles::OpenFile,
        timeout: Option<Duration>,
        term_mode: Option<brush_core::terminal::AutoModeGuard>,
        output: &'a mut dyn std::io::Write,
    ) -> Self {
        Self {
            input,
            deadline: timeout.map(|t| Instant::now() + t),
            buffer: [0; 1],
            _term_mode: term_mode,
            output,
        }
    }

    /// Displays a prompt if provided.
    fn display_prompt(&mut self, prompt: Option<&str>) -> Result<(), brush_core::Error> {
        if let Some(prompt) = prompt {
            write!(self.output, "{prompt}")?;
            self.output.flush()?;
        }
        Ok(())
    }

    /// Checks if input is immediately available (for `-t 0`).
    fn check_input_available(&self) -> bool {
        brush_core::sys::poll::poll_for_input(&self.input, Duration::ZERO).unwrap_or(false)
    }

    /// Reads the next input event, handling timeout and control characters.
    fn read_event(&mut self) -> Result<InputEvent, brush_core::Error> {
        // Check timeout before attempting read.
        if let Some(deadline) = self.deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(InputEvent::Timeout);
            }

            // Poll for input with remaining timeout.
            match brush_core::sys::poll::poll_for_input(&self.input, remaining) {
                Ok(true) => { /* Data available, proceed. */ }
                Ok(false) => return Ok(InputEvent::Timeout),
                Err(e) => return Err(e.into()),
            }
        }

        let n = self.input.read(&mut self.buffer)?;
        if n == 0 {
            return Ok(InputEvent::Eof);
        }

        let ch = self.buffer[0] as char;

        // Map control characters to events.
        Ok(match ch {
            CTRL_C => InputEvent::CtrlC,
            CTRL_D => InputEvent::CtrlD,
            _ => InputEvent::Char(ch),
        })
    }
}

/// Configuration for line reading behavior.
struct LineReaderConfig {
    /// Character that terminates input (None for -N mode).
    delimiter: Option<char>,
    /// Maximum characters to read (for -n or -N).
    char_limit: Option<usize>,
    /// Whether to process backslash escapes (false for -r mode).
    process_escapes: bool,
}

/// Reads a complete line of input using the given reader and configuration.
///
/// Returns a `ReadResult` indicating success, EOF, timeout, or interruption.
fn read_line_with_reader(
    reader: &mut InputReader<'_>,
    config: &LineReaderConfig,
) -> Result<ReadResult, brush_core::Error> {
    let mut line = String::new();
    let mut pending_backslash = false;

    loop {
        let event = reader.read_event()?;

        match event {
            InputEvent::Eof => {
                // Include pending backslash on EOF.
                if pending_backslash {
                    line.push(BACKSLASH);
                }
                return Ok(ReadResult::Eof(if line.is_empty() {
                    None
                } else {
                    Some(line)
                }));
            }

            InputEvent::Timeout => {
                // Include pending backslash on timeout.
                if pending_backslash {
                    line.push(BACKSLASH);
                }
                return Ok(ReadResult::TimedOut(if line.is_empty() {
                    None
                } else {
                    Some(line)
                }));
            }

            InputEvent::CtrlC => {
                return Ok(ReadResult::Interrupted);
            }

            InputEvent::CtrlD => {
                // Include pending backslash before checking if line is empty.
                if pending_backslash {
                    line.push(BACKSLASH);
                }
                // At line start = EOF, mid-input = flush current input.
                return Ok(if line.is_empty() {
                    ReadResult::Eof(None)
                } else {
                    ReadResult::Line(line)
                });
            }

            InputEvent::Char(ch) => {
                // Handle backslash escape processing (when enabled).
                if config.process_escapes {
                    if pending_backslash {
                        pending_backslash = false;

                        // Backslash-delimiter is line continuation.
                        if let Some(delim) = config.delimiter {
                            if ch == delim {
                                continue; // Line continuation.
                            }
                        }

                        // For other chars, remove backslash, add char literally.
                        line.push(ch);

                        // Check character limit.
                        if let Some(limit) = config.char_limit {
                            if line.len() >= limit {
                                return Ok(ReadResult::Line(line));
                            }
                        }
                        continue;
                    }

                    if ch == BACKSLASH {
                        pending_backslash = true;
                        continue;
                    }
                }

                // Check for delimiter.
                if let Some(delim) = config.delimiter {
                    if ch == delim {
                        // Include pending backslash if we had one.
                        if pending_backslash {
                            line.push(BACKSLASH);
                        }
                        return Ok(ReadResult::Line(line));
                    }
                }

                // Ignore non-whitespace control characters.
                if ch.is_ascii_control() && !ch.is_ascii_whitespace() {
                    continue;
                }

                line.push(ch);

                // Check character limit.
                if let Some(limit) = config.char_limit {
                    if line.len() >= limit {
                        // Include pending backslash if we had one.
                        if pending_backslash {
                            line.push(BACKSLASH);
                        }
                        return Ok(ReadResult::Line(line));
                    }
                }
            }
        }
    }
}

impl ReadCommand {
    /// Reads a line of input, optionally with a timeout.
    ///
    /// Handles backslash escape processing:
    /// - Without `-r`: backslash-newline is line continuation, other backslashes escape the next char
    /// - With `-r`: backslash is treated as a literal character
    fn read_line(
        &self,
        input_file: brush_core::openfiles::OpenFile,
        mut output_file: impl std::io::Write,
        timeout: Option<Duration>,
    ) -> Result<ReadResult, brush_core::Error> {
        let term_mode = self.setup_terminal_settings(&input_file)?;

        // Determine delimiter based on options.
        let delimiter = if self.return_after_n_chars_no_delimiter.is_some() {
            None
        } else if let Some(delimiter_str) = &self.delimiter {
            if delimiter_str.is_empty() {
                Some(NUL_DELIMITER)
            } else {
                delimiter_str.chars().next()
            }
        } else {
            Some(DEFAULT_DELIMITER)
        };

        let char_limit = self
            .return_after_n_chars_no_delimiter
            .or(self.return_after_n_chars);

        // Create the input reader.
        let mut reader = InputReader::new(input_file, timeout, term_mode, &mut output_file);

        // Display prompt if provided.
        reader.display_prompt(self.prompt.as_deref())?;

        // Handle -t 0 special case: just check if input is available without reading.
        if timeout == Some(Duration::ZERO) {
            return Ok(if reader.check_input_available() {
                ReadResult::InputReady
            } else {
                ReadResult::InputNotReady
            });
        }

        // Configure and perform the read.
        let config = LineReaderConfig {
            delimiter,
            char_limit,
            process_escapes: !self.raw_mode,
        };

        read_line_with_reader(&mut reader, &config)
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
    let mut ended_with_non_ws_delimiter = false;
    let mut in_last_field = false;

    for c in trimmed_line.chars() {
        if skipping_ifs_whitespace {
            if ifs_space_chars.contains(&c) {
                continue;
            }

            skipping_ifs_whitespace = false;
        }

        // Check if we've reached max_fields and should start the last field.
        let at_max_fields = fields.len() + 1 >= max_fields;

        if !at_max_fields && ifs_chars.contains(&c) {
            fields.push_back(current_field);
            current_field = String::new();
            skipping_ifs_whitespace = ifs_space_chars.contains(&c);
            ended_with_non_ws_delimiter = !skipping_ifs_whitespace;
        } else if at_max_fields && !in_last_field && ifs_chars.contains(&c) {
            // We've hit max_fields but haven't started the last field content yet.
            // Skip this delimiter. If it's whitespace, continue skipping whitespace.
            if ifs_space_chars.contains(&c) {
                skipping_ifs_whitespace = true;
            }
            // For non-whitespace delimiters at field boundary, just skip this one char.
        } else {
            in_last_field = at_max_fields;
            current_field.push(c);
            ended_with_non_ws_delimiter = false;
        }
    }

    // Push the final field. However, bash does not include an empty trailing field
    // when the input ends with a non-whitespace delimiter.
    // e.g., "a,b,c," with IFS="," gives ["a", "b", "c"], not ["a", "b", "c", ""].
    if !current_field.is_empty() || !ended_with_non_ws_delimiter {
        fields.push_back(current_field);
    }

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
        // Bash does NOT include empty trailing field when input ends with non-ws delimiter.
        let result = split_line_by_ifs(",", "a,b,c,", None);
        assert_equal(result, VecDeque::from(vec!["a", "b", "c"]));
    }
}
