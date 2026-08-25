use std::io::{Read, Write};

use brush_core::{ErrorKind, ExecutionExitCode, ExecutionResult, builtins, env, error, variables};

/// Read lines from standard input into an indexed array variable.
pub(crate) struct MapFileCommand {
    /// Delimiter to use (defaults to newline).
    delimiter: Option<String>,

    /// Maximum number of entries to read (0 means no limit).
    max_count: i64,

    /// Index into array at which to start assignment.
    origin: Option<i64>,

    /// Number of initial entries to skip.
    skip_count: i64,

    /// Whether or not to remove the delimiter from each read line.
    remove_delimiter: bool,

    /// File descriptor to read from (defaults to stdin).
    fd: brush_core::ShellFd,

    /// Name of function to call for each group of lines.
    callback: Option<String>,

    /// Number of lines to pass the callback for each group.
    callback_group_size: i64,

    /// Name of array to read into.
    array_var_name: String,
}

const ID_DELIMITER: &str = "delimiter";
const ID_MAX_COUNT: &str = "max_count";
const ID_ORIGIN: &str = "origin";
const ID_SKIP_COUNT: &str = "skip_count";
const ID_REMOVE_DELIMITER: &str = "remove_delimiter";
const ID_FD: &str = "fd";
const ID_CALLBACK: &str = "callback";
const ID_CALLBACK_GROUP_SIZE: &str = "callback_group_size";
const ID_ARRAY_VAR_NAME: &str = "array_var_name";

impl builtins::SpecCommand for MapFileCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_DELIMITER,
            &['d'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("DELIM"),
            "Delimiter to use (defaults to newline).",
        )
        .arg(
            ID_MAX_COUNT,
            &['n'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("COUNT"),
            "Maximum number of entries to read (0 means no limit).",
        )
        .arg(
            ID_ORIGIN,
            &['O'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("ORIGIN"),
            "Index into array at which to start assignment.",
        )
        .arg(
            ID_SKIP_COUNT,
            &['s'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("COUNT"),
            "Number of initial entries to skip.",
        )
        .arg(
            ID_REMOVE_DELIMITER,
            &['t'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Whether or not to remove the delimiter from each read line.",
        )
        .arg(
            ID_FD,
            &['u'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("FD"),
            "File descriptor to read from (defaults to stdin).",
        )
        .arg(
            ID_CALLBACK,
            &['C'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("CALLBACK"),
            "Name of function to call for each group of lines.",
        )
        .arg(
            ID_CALLBACK_GROUP_SIZE,
            &['c'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("COUNT"),
            "Number of lines to pass the callback for each group.",
        )
        .positional(ID_ARRAY_VAR_NAME, "ARRAY_VAR_NAME")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let delimiter = matches.value(ID_DELIMITER).map(str::to_string);
        let max_count = match matches.value(ID_MAX_COUNT) {
            Some(v) => parse_i64(v)?,
            None => 0,
        };
        let origin = match matches.value(ID_ORIGIN) {
            Some(v) => Some(parse_i64(v)?),
            None => None,
        };
        let skip_count = match matches.value(ID_SKIP_COUNT) {
            Some(v) => {
                let parsed = parse_i64(v)?;
                if parsed < 0 {
                    return Err(builtins::BuiltinArgParseError {
                        message: format!("-s: must be >= 0: {v}"),
                        help_request: false,
                    });
                }
                parsed
            }
            None => 0,
        };
        let remove_delimiter = matches.flag(ID_REMOVE_DELIMITER);
        let fd = match matches.value(ID_FD) {
            Some(v) => v
                .parse::<brush_core::ShellFd>()
                .map_err(|_| invalid_number(v))?,
            None => 0,
        };
        let callback = matches.value(ID_CALLBACK).map(str::to_string);
        let callback_group_size = match matches.value(ID_CALLBACK_GROUP_SIZE) {
            Some(v) => {
                let parsed = parse_i64(v)?;
                if parsed < 1 {
                    return Err(builtins::BuiltinArgParseError {
                        message: format!("-c: must be >= 1: {v}"),
                        help_request: false,
                    });
                }
                parsed
            }
            None => 5000,
        };
        let array_var_name = matches
            .value(ID_ARRAY_VAR_NAME)
            .map_or_else(|| String::from("MAPFILE"), str::to_string);

        Ok(Self {
            delimiter,
            max_count,
            origin,
            skip_count,
            remove_delimiter,
            fd,
            callback,
            callback_group_size,
            array_var_name,
        })
    }

    fn about() -> &'static str {
        "Read lines from standard input into an indexed array variable."
    }

    fn synopsis() -> &'static str {
        "[-d DELIM] [-n COUNT] [-O ORIGIN] [-s COUNT] [-t] [-u FD] [-C CALLBACK] [-c COUNT] [ARRAY_VAR_NAME]"
    }

    fn value_taking_short_options() -> &'static str {
        "dnOsuCc"
    }

    // N.B. Overrides the default [`builtins::SpecCommand::new`] so that a flag-looking
    // value for `-O` (e.g., `mapfile -O -3`, a negative array origin) gets joined
    // into `-O=-3`; the backend otherwise rejects separate flag-shaped values.
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }
        join_tokens_taking_values(&mut args, "O");

        let spec = Self::declare(builtins::argmodel::CommandSpecBuilder::new()).build();
        let mut matches = builtins::argmodel::backend().parse(&spec, "", &args)?;

        Self::from_matches(&mut matches)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.callback_group_size != 5000 || self.callback.is_some() {
            return error::unimp("mapfile -C/-c is not yet implemented");
        }

        if let Some(origin) = self.origin {
            if origin < 0 {
                writeln!(
                    context.stderr(),
                    "{}: {origin}: invalid array origin",
                    context.command_name
                )?;
                return Ok(ExecutionExitCode::GeneralError.into());
            }
        }

        if let Some((_, var)) = context.shell.env().get(&self.array_var_name) {
            if var.value().is_associative_array() {
                writeln!(
                    context.stderr(),
                    "{}: {}: not an indexed array",
                    context.command_name,
                    self.array_var_name
                )?;
                return Ok(ExecutionExitCode::GeneralError.into());
            }
        }

        let input_file = context
            .try_fd(self.fd)
            .ok_or_else(|| ErrorKind::BadFileDescriptor(self.fd))?;

        // Read!
        let results = self.read_entries(input_file)?;

        if let Some(origin) = self.origin {
            // -O: preserve existing array, assign at offset.
            for (elem_idx, (_key, value)) in results.0.into_iter().enumerate() {
                // If the user is getting to wraparounds in *bash*, they got bigger problems.
                #[allow(clippy::cast_possible_wrap)]
                let elem_idx = elem_idx as i64;
                context.shell.env_mut().update_or_add_array_element(
                    &self.array_var_name,
                    (elem_idx + origin).to_string(),
                    value,
                    |_| Ok(()),
                    env::EnvironmentLookup::Anywhere,
                    env::EnvironmentScope::Global,
                )?;
            }
        } else {
            // No -O: replace the entire variable (clears existing).
            context.shell.env_mut().update_or_add(
                &self.array_var_name,
                variables::ShellValueLiteral::Array(results),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        }

        Ok(ExecutionResult::success())
    }
}

impl MapFileCommand {
    fn read_entries(
        &self,
        mut input_file: brush_core::openfiles::OpenFile,
    ) -> Result<variables::ArrayLiteral, brush_core::Error> {
        let _term_mode = setup_terminal_settings(&input_file)?;

        let mut entries = vec![];
        let mut read_count = 0;
        let max_count = self.max_count.try_into()?;
        let delimiter = match &self.delimiter {
            Some(d) if d.is_empty() => b'\0',
            Some(d) => d.as_bytes().first().copied().unwrap_or(b'\n'),
            None => b'\n',
        };

        let mut buf = [0u8; 1];

        while max_count == 0 || entries.len() < max_count {
            let mut line = vec![];
            let mut saw_delimiter = false;

            loop {
                match input_file.read(&mut buf) {
                    Ok(0) => break,                                         // End of input
                    Ok(1) if buf[0] == b'\x03' => break,                    // Ctrl+C
                    Ok(1) if buf[0] == b'\x04' && line.is_empty() => break, // Ctrl+D
                    Ok(1) => {
                        let byte = buf[0];
                        line.push(byte);
                        if byte == delimiter {
                            saw_delimiter = true;
                            break;
                        }
                    }
                    Ok(_) => unreachable!("input can only be 0, 1, or error"),
                    Err(e) => return Err(e.into()),
                }
            }

            if line.is_empty() && !saw_delimiter {
                break;
            }

            if read_count < self.skip_count {
                read_count += 1;
                continue;
            }

            if self.remove_delimiter && line.ends_with(&[delimiter]) {
                line.pop();
            }

            let line_str = String::from_utf8_lossy(&line).to_string();

            entries.push((None, line_str));
        }

        Ok(variables::ArrayLiteral(entries))
    }
}

fn setup_terminal_settings(
    file: &brush_core::openfiles::OpenFile,
) -> Result<Option<brush_core::terminal::AutoModeGuard>, brush_core::Error> {
    let mode = brush_core::terminal::AutoModeGuard::new(file.to_owned()).ok();
    if let Some(mode) = &mode {
        let config = brush_core::terminal::Settings::builder()
            .line_input(false)
            .interrupt_signals(false)
            .build();

        mode.apply_settings(&config)?;
    }

    Ok(mode)
}

/// Merges `-X` tokens followed by a flag-looking value token into `-X=<value>`
/// so that the argument backend accepts values that would otherwise be
/// rejected as flags;
/// e.g., negative numbers.
fn join_tokens_taking_values(args: &mut Vec<String>, shorts: &str) {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].clone();

        if arg == "--" {
            break;
        }

        let takes_value = arg.len() == 2
            && arg.starts_with('-')
            && arg.chars().nth(1).is_some_and(|c| shorts.contains(c));

        if takes_value {
            if let Some(next) = args.get(i + 1) {
                if next.starts_with('-') && next != "-" && next != "--" {
                    args[i] = format!("{arg}={next}");
                    args.remove(i + 1);
                }
            }
        }

        i += 1;
    }
}

/// Parses an `i64` option value, reporting a parse failure on invalid input.
fn parse_i64(value: &str) -> Result<i64, builtins::BuiltinArgParseError> {
    value.parse::<i64>().map_err(|_| invalid_number(value))
}

fn invalid_number(value: &str) -> builtins::BuiltinArgParseError {
    builtins::BuiltinArgParseError {
        message: format!("invalid number: {value}"),
        help_request: false,
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use brush_core::builtins::SpecCommand as _;

    fn new_from(args: &[&str]) -> Result<MapFileCommand, builtins::BuiltinArgParseError> {
        MapFileCommand::new(
            std::iter::once("mapfile".to_string()).chain(args.iter().map(|s| s.to_string())),
        )
    }

    #[test]
    fn test_defaults() -> anyhow::Result<()> {
        let cmd = new_from(&[])?;
        assert_eq!(cmd.max_count, 0);
        assert_eq!(cmd.skip_count, 0);
        assert_eq!(cmd.fd, 0);
        assert_eq!(cmd.callback_group_size, 5000);
        assert_eq!(cmd.array_var_name, "MAPFILE");
        assert_eq!(cmd.origin, None);
        Ok(())
    }

    #[test]
    fn test_negative_origin_separate_token() -> anyhow::Result<()> {
        let cmd = new_from(&["-O", "-3"])?;
        assert_eq!(cmd.origin, Some(-3));
        Ok(())
    }

    #[test]
    fn test_options_with_array_name() -> anyhow::Result<()> {
        let cmd = new_from(&["-t", "-u", "1", "-s", "2", "-n", "10", "myarray"])?;
        assert!(cmd.remove_delimiter);
        assert_eq!(cmd.fd, 1);
        assert_eq!(cmd.skip_count, 2);
        assert_eq!(cmd.max_count, 10);
        assert_eq!(cmd.array_var_name, "myarray");
        Ok(())
    }

    #[test]
    fn test_invalid_skip_count_rejected() {
        assert!(new_from(&["-s", "-1"]).is_err());
    }
}
