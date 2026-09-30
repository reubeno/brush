use std::{ffi::OsString, io::Write, ops::ControlFlow};
use usage::Cli;
use uucore::format;

use brush_core::{Error, ErrorKind, ExecutionResult, builtins, escape, expansion};

/// Format a string.
#[derive(Cli)]
#[usage(
    bin = "printf",
    unknown_flags = "error",
    args_override_self = false,
    disable_help_flag,
    disable_version_flag
)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[usage(short = 'v')]
    output_variable: Option<String>,

    /// Format string + arguments to the format string.
    ///
    /// N.B. We intentionally do *not* enable `allow_hyphen_values` here. Doing so would
    /// cause an attached short-option value such as `-va` (i.e. `-v a`) to be misparsed as
    /// a positional argument. With it disabled, a format string that genuinely needs to
    /// start with a hyphen must be preceded by `--`, matching other shells' behavior.
    #[usage(trailing_var_arg, required)]
    format_and_args: Vec<String>,
}

impl brush_builtin_usage::UsageParsed for PrintfCommand {
    fn parse_argv<'v>(argv: &[&'v std::ffi::OsStr]) -> Result<Self, usage::Error<'static, 'v>> {
        Self::parse_from(argv)
    }

    fn spec() -> &'static usage::spec::Spec<'static> {
        Self::spec()
    }

    fn command() -> &'static usage::Command<'static> {
        Self::command()
    }
}

impl brush_core::builtins::FromArgs for PrintfCommand {
    fn from_args(
        name: &str,
        args: Vec<brush_core::CommandArg>,
    ) -> Result<Self, brush_core::builtins::ArgsError> {
        // Bash accepts only leading `-v` options and drops at most one `--`
        // that ends them. Every later token, including further `--`s and words
        // that look like flags, is format data.
        let words = brush_builtin_utils::into_words(args);
        let mut output_variable = None;
        let mut index = 0;

        while let Some(word) = words.get(index) {
            if word == "--" {
                index += 1;
                break;
            } else if word == "-v" {
                index += 1;
                match words.get(index) {
                    Some(value) => {
                        output_variable = Some(value.clone());
                        index += 1;
                    }
                    None => {
                        return Err(brush_core::builtins::ArgsError::Usage(format!(
                            "{name}: -v: option requires an argument\n"
                        )));
                    }
                }
            } else if let Some(value) = word.strip_prefix("-v")
                && !value.is_empty()
            {
                output_variable = Some(value.to_owned());
                index += 1;
            } else if word.starts_with('-') && word != "-" {
                let flag = word.chars().nth(1).unwrap_or('-');
                return Err(brush_core::builtins::ArgsError::Usage(format!(
                    "{name}: invalid option -- '{flag}'\n"
                )));
            } else {
                break;
            }
        }

        let format_and_args = words[index..].to_vec();
        if format_and_args.is_empty() {
            return Err(brush_core::builtins::ArgsError::Usage(format!(
                "{name}: usage: printf [-v var] format [arguments]\n"
            )));
        }

        Ok(Self {
            output_variable,
            format_and_args,
        })
    }
}

brush_builtin_usage::__usage_builtin_help!(PrintfCommand);

impl builtins::Command for PrintfCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if let Some(variable_name) = &self.output_variable {
            // Format to a u8 vector.
            let mut result: Vec<u8> = vec![];
            format(self.format_and_args.as_slice(), &mut result)?;

            // Convert to a string.
            let result_str = String::from_utf8(result).map_err(|_| {
                brush_core::ErrorKind::PrintfInvalidUsage("invalid UTF-8 output".into())
            })?;

            // Assign to the selected variable.
            expansion::assign_to_named_parameter(
                context.shell,
                &context.params,
                variable_name,
                result_str,
            )
            .await?;
        } else {
            format(self.format_and_args.as_slice(), context.stdout())?;
            context.stdout().flush()?;
        }

        Ok(ExecutionResult::success())
    }
}

fn format(format_and_args: &[String], writer: impl Write) -> Result<(), brush_core::Error> {
    match format_and_args {
        // Handle format string with arguments using uucore
        [fmt, args @ ..] => format_via_uucore(fmt, args.iter(), writer),
        // Handle case with no format string (we shouldn't be able to get here since parsing
        // fails when the format string is missing)
        [] => Err(ErrorKind::PrintfInvalidUsage("missing operand".into()).into()),
    }
}

fn format_via_uucore(
    format_string: &str,
    args: impl Iterator<Item = impl Into<OsString>>,
    mut writer: impl Write,
) -> Result<(), brush_core::Error> {
    // Convert string arguments to FormatArgument::Unparsed
    let format_args: Vec<_> = args
        .map(|s| format::FormatArgument::Unparsed(s.into()))
        .collect();

    // Parse format string once.
    let format_items = parse_format_string(format_string)?;

    // Wrap the format arguments.
    let mut format_args_wrapper = format::FormatArguments::new(&format_args);

    // Determine whether the format string contains any specifiers that consume arguments. If it
    // doesn't, then we must only run through it once -- even when extra arguments are provided --
    // since otherwise we'd loop forever waiting for arguments that will never be consumed. This
    // matches the behavior of other shells, which print such a format string exactly once.
    let format_consumes_args = format_items
        .iter()
        .any(|(item, _)| matches!(item, format::FormatItem::Spec(_)));

    // Keep going until we've exhausted all format arguments. Also make sure to run at least once
    // even if there's no format arguments.
    while format_args.is_empty() || !format_args_wrapper.is_exhausted() {
        // Process all format items, in order. We'll bail when we're told to stop.
        for (item, backslash_quote) in &format_items {
            if let (format::FormatItem::Spec(format::Spec::QuotedString { position }), true) =
                (item, *backslash_quote)
            {
                let arg = format_args_wrapper.next_string(*position).to_string_lossy();
                let quoted = quote_printf_q(&arg);
                write!(writer, "{quoted}")?;
                continue;
            }

            let control_flow = item
                .write(&mut writer, &mut format_args_wrapper)
                .map_err(|e| match e {
                    // Propagate I/O errors directly so they can be handled appropriately
                    format::FormatError::IoError(io_err) => Error::from(io_err),
                    // Wrap other format errors
                    other => Error::from(ErrorKind::PrintfInvalidUsage(std::format!(
                        "printf formatting error: {other}"
                    ))),
                })?;

            if control_flow == ControlFlow::Break(()) {
                break;
            }
        }

        // If the format string doesn't consume any arguments, stop now; otherwise we'd reprocess
        // it forever since no arguments will ever be consumed.
        if !format_consumes_args {
            break;
        }

        // Start next batch if not exhausted
        if !format_args_wrapper.is_exhausted() {
            format_args_wrapper.start_next_batch();
        }

        if format_args.is_empty() {
            break;
        }
    }

    Ok(())
}

fn quote_printf_q(s: &str) -> String {
    let quoted = escape::quote_if_needed(s, escape::QuoteMode::BackslashEscape);
    if quoted.starts_with("$'") {
        return quoted.into_owned();
    }

    let mut quoted = quoted.replace(":~", ":\\~").replace("=~", "=\\~");
    if matches!(quoted.as_bytes().first(), Some(b'~' | b'#')) {
        quoted.insert(0, '\\');
    }
    quoted
}

type ParsedFormatItem = (format::FormatItem<format::EscapedChar>, bool);

fn parse_format_string(format_string: &str) -> Result<Vec<ParsedFormatItem>, brush_core::Error> {
    let format_items: Result<Vec<_>, _> = format::parse_spec_and_escape(format_string.as_bytes())
        .map(|result| match result {
            Ok(item @ format::FormatItem::Spec(format::Spec::QuotedString { .. })) => {
                Ok((item, true))
            }
            Ok(item) => Ok((item, false)),
            // Fixed q/Q modifiers are deliberately ignored; dynamic modifiers remain unsupported
            // until uucore exposes quoted-string metadata.
            Err(format::FormatError::SpecError(spec, span))
                if matches!(spec.last(), Some(b'q' | b'Q'))
                    && !spec.contains(&b'*')
                    && !spec.contains(&b'$') =>
            {
                let mut bare_q: &[u8] = b"q";
                let item = format::Spec::parse(&mut bare_q)
                    .map(format::FormatItem::Spec)
                    .map_err(|spec| format::FormatError::SpecError(spec.to_vec(), span))?;
                Ok((item, !spec.contains(&b'#')))
            }
            Err(error) => Err(error),
        })
        .collect();

    // Observe any errors we encountered along the way.
    let format_items = format_items
        .map_err(|e| ErrorKind::PrintfInvalidUsage(format!("printf parsing error: {e}")))?;

    Ok(format_items)
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn sprintf_via_uucore(
        format_string: &str,
        args: impl Iterator<Item = impl Into<OsString>>,
    ) -> Result<String> {
        let mut result = vec![];
        format_via_uucore(format_string, args, &mut result)?;

        Ok(String::from_utf8(result)?)
    }

    #[test]
    fn test_basic_sprintf() -> Result<()> {
        assert_eq!(sprintf_via_uucore("%s", std::iter::once(&"xyz"))?, "xyz");
        assert_eq!(sprintf_via_uucore(r"%d\n", std::iter::once(&"1"))?, "1\n");

        Ok(())
    }

    #[test]
    fn test_sprintf_without_args() -> Result<()> {
        let empty: [&str; 0] = [];

        assert_eq!(sprintf_via_uucore("xyz", empty.iter())?, "xyz");
        assert_eq!(sprintf_via_uucore("%s|", empty.iter())?, "|");

        Ok(())
    }

    #[test]
    fn test_sprintf_with_cycles() -> Result<()> {
        assert_eq!(sprintf_via_uucore("%s|", ["x", "y"].iter())?, "x|y|");

        Ok(())
    }
}
