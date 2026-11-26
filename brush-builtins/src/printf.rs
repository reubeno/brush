use std::{ffi::OsString, io::Write, ops::ControlFlow};
use uucore::format;

use brush_core::{Error, ErrorKind, ExecutionResult, builtins, escape, expansion};

/// Format a string.
#[derive(winnow_args::Args)]
#[arg(disable_help_flag, disable_version_flag, disable_help_subcommand)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[arg(short = 'v', allow_hyphen_values)]
    output_variables: Vec<String>,

    /// Format string + arguments to the format string.
    #[arg(positional, stop_flags, required)]
    format_and_args: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(PrintfCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for PrintfCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // Every -v name must be valid, not just the last one, which is used.
        let parser_options = context.shell.parser_options();
        if let Some(invalid) = self
            .output_variables
            .iter()
            .find(|name| !valid_output_variable(name, &parser_options))
        {
            writeln!(
                context.stderr(),
                "{}: `{invalid}': not a valid identifier",
                context.command_name
            )?;
            return Ok(ExecutionResult::new(2));
        }

        if let Some(variable_name) = self.output_variables.last() {
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

/// Whether `name` is a variable name, or one followed by a non-empty `[subscript]`,
/// by the grammar of an assignment's name. The subscript isn't evaluated here.
fn valid_output_variable(name: &str, options: &brush_parser::ParserOptions) -> bool {
    use brush_parser::ast::{AssignmentName, AssignmentValue};
    match brush_parser::word::parse_scalar_assignment(&format!("{name}="), options) {
        Ok(assignment) if !assignment.append => {
            matches!(&assignment.value, AssignmentValue::Scalar(word) if word.value.is_empty())
                && match assignment.name {
                    AssignmentName::VariableName(_) => true,
                    AssignmentName::ArrayElementName(_, index) => !index.is_empty(),
                }
        }
        _ => false,
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
        for (item, quote_spec) in &format_items {
            if let (
                format::FormatItem::Spec(format::Spec::QuotedString { position }),
                Some(quote_spec),
            ) = (item, quote_spec)
            {
                let arg = format_args_wrapper.next_string(*position).to_string_lossy();
                writer.write_all(&quote_spec.format(&arg))?;
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
    // Like bash's, this escapes commas too, so the result is safe even inside a brace
    // expansion.
    let options = escape::QuoteOptions::builder()
        .preferred_mode(escape::QuoteMode::BackslashEscape)
        .quote_commas(true)
        .build();
    escape::quote(s, &options).into_owned()
}

/// A `%q` or `%Q` conversion with its fixed flags, width and precision, which uucore
/// doesn't accept for these conversions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct QuoteSpec {
    /// `%Q`: the precision applies to the argument before it's quoted.
    upper: bool,
    /// The `#` flag: quote with single quotes, even when nothing needs quoting.
    alternate: bool,
    /// The `-` flag: pad on the right.
    left_align: bool,
    width: usize,
    precision: Option<usize>,
}

impl QuoteSpec {
    /// Parses the bytes after the `%` of a fixed `q` or `Q` spec, e.g. `-12.5q`. Like bash,
    /// accepts and ignores flags other than `-` and `#`, and any length modifiers.
    fn parse(spec: &[u8]) -> Option<Self> {
        let mut result = Self::default();
        let mut rest = spec;

        while let [flag @ (b'#' | b'\'' | b'-' | b'+' | b' ' | b'0'), tail @ ..] = rest {
            match flag {
                b'#' => result.alternate = true,
                b'-' => result.left_align = true,
                _ => (),
            }
            rest = tail;
        }

        result.width = parse_decimal(&mut rest)?;

        if let [b'.', tail @ ..] = rest {
            rest = tail;
            // A precision with no digits is zero.
            result.precision = Some(parse_decimal(&mut rest)?);
        }

        while let [b'h' | b'j' | b'l' | b'L' | b't' | b'z', tail @ ..] = rest {
            rest = tail;
        }

        result.upper = match rest {
            [b'q'] => false,
            [b'Q'] => true,
            _ => return None,
        };

        Some(result)
    }

    /// Quotes `arg`, then applies the precision and width, both counted in bytes as bash does.
    fn format(&self, arg: &str) -> Vec<u8> {
        let arg = match self.precision {
            Some(precision) if self.upper => truncate_at_char_boundary(arg, precision),
            _ => arg,
        };

        let quoted = if self.alternate {
            escape::force_quote(arg, escape::QuoteMode::SingleQuote)
        } else {
            quote_printf_q(arg)
        };

        let mut quoted = quoted.into_bytes();
        if let Some(precision) = self.precision.filter(|_| !self.upper) {
            quoted.truncate(precision);
        }

        let padding = vec![b' '; self.width.saturating_sub(quoted.len())];
        if self.left_align {
            quoted.extend(padding);
            quoted
        } else {
            [padding, quoted].concat()
        }
    }
}

/// Parses the run of decimal digits at the start of `rest` and advances past it. No digits
/// count as zero. Returns `None` if the number is larger than bash allows (`INT_MAX`).
fn parse_decimal(rest: &mut &[u8]) -> Option<usize> {
    const MAX: usize = 0x7fff_ffff;

    let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    let (number, tail) = rest.split_at(digits);
    *rest = tail;
    number
        .iter()
        .try_fold(0usize, |value, digit| {
            value
                .checked_mul(10)?
                .checked_add(usize::from(digit - b'0'))
        })
        .filter(|value| *value <= MAX)
}

/// Returns the longest prefix of `s` that is at most `max_len` bytes and ends on a char
/// boundary.
fn truncate_at_char_boundary(s: &str, max_len: usize) -> &str {
    if max_len >= s.len() {
        return s;
    }

    let mut end = max_len;
    while !s.is_char_boundary(end) {
        end -= 1;
    }

    s.get(..end).unwrap_or_default()
}

type ParsedFormatItem = (format::FormatItem<format::EscapedChar>, Option<QuoteSpec>);

fn parse_format_string(format_string: &str) -> Result<Vec<ParsedFormatItem>, brush_core::Error> {
    let format_items: Result<Vec<_>, _> = format::parse_spec_and_escape(format_string.as_bytes())
        .map(|result| match result {
            Ok(item @ format::FormatItem::Spec(format::Spec::QuotedString { .. })) => {
                Ok((item, Some(QuoteSpec::default())))
            }
            Ok(item) => Ok((item, None)),
            // uucore rejects q/Q with flags, a width or a precision, so fixed ones are handled
            // here. Dynamic (`*`) and positional (`$`) forms remain unsupported.
            Err(format::FormatError::SpecError(spec, span)) => {
                let Some(quote_spec) = QuoteSpec::parse(&spec) else {
                    return Err(format::FormatError::SpecError(spec, span));
                };
                let mut bare_q: &[u8] = b"q";
                let item = format::Spec::parse(&mut bare_q)
                    .map(format::FormatItem::Spec)
                    .map_err(|spec| format::FormatError::SpecError(spec.to_vec(), span))?;
                Ok((item, Some(quote_spec)))
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

    #[test]
    fn test_sprintf_quoted_with_width_and_precision() -> Result<()> {
        assert_eq!(
            sprintf_via_uucore("<%12q>", ["foo bar"].iter())?,
            "<    foo\\ bar>"
        );
        assert_eq!(
            sprintf_via_uucore("<%-12q>", ["foo bar"].iter())?,
            "<foo\\ bar    >"
        );
        assert_eq!(
            sprintf_via_uucore("<%.5q>", ["foo bar"].iter())?,
            "<foo\\ >"
        );
        assert_eq!(
            sprintf_via_uucore("<%.5Q>", ["foo bar"].iter())?,
            "<foo\\ b>"
        );
        assert_eq!(sprintf_via_uucore("<%.q>", ["x"].iter())?, "<>");
        assert_eq!(sprintf_via_uucore("<%3q>", [""].iter())?, "< ''>");
        assert_eq!(sprintf_via_uucore("<%#q>", ["foo"].iter())?, "<'foo'>");
        assert_eq!(
            sprintf_via_uucore("<%#12q>", ["foo bar"].iter())?,
            "<   'foo bar'>"
        );

        Ok(())
    }

    #[test]
    fn test_parse_quote_spec() {
        assert_eq!(QuoteSpec::parse(b"q"), Some(QuoteSpec::default()));
        assert_eq!(
            QuoteSpec::parse(b"-#012.3lQ"),
            Some(QuoteSpec {
                upper: true,
                alternate: true,
                left_align: true,
                width: 12,
                precision: Some(3),
            })
        );
        assert_eq!(QuoteSpec::parse(b"*q"), None);
        assert_eq!(QuoteSpec::parse(b"2$q"), None);
        assert_eq!(QuoteSpec::parse(b"5s"), None);
        assert_eq!(QuoteSpec::parse(b"99999999999999999999999q"), None);
        assert_eq!(QuoteSpec::parse(b"99999999999q"), None);
    }
}
