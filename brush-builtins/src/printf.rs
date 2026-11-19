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

        let errors = if let Some(variable_name) = self.output_variables.last() {
            // Format to a u8 vector.
            let mut result: Vec<u8> = vec![];
            let errors = format(self.format_and_args.as_slice(), &mut result)?;

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
            errors
        } else {
            let errors = format(self.format_and_args.as_slice(), context.stdout())?;
            context.stdout().flush()?;
            errors
        };

        for error in &errors {
            writeln!(context.stderr(), "{}: {error}", context.command_name)?;
        }
        Ok(ExecutionResult::new(u8::from(!errors.is_empty())))
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

fn format(
    format_and_args: &[String],
    writer: impl Write,
) -> Result<Vec<String>, brush_core::Error> {
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
) -> Result<Vec<String>, brush_core::Error> {
    // Convert string arguments to FormatArgument::Unparsed
    let format_args: Vec<_> = args
        .map(|s| format::FormatArgument::Unparsed(s.into()))
        .collect();

    // Parse format string once.
    let (format_string, modifiers) = prepare_modifiers(format_string);
    let format_items = parse_format_string(&format_string)?;
    let mut errors = vec![];

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
        let mut modifiers = modifiers.iter();
        for (item, backslash_quote) in &format_items {
            let resolved;
            let item = if matches!(item, format::FormatItem::Spec(_)) {
                if let Some(Some(modifiers)) = modifiers.next() {
                    let Some(spec) = modifiers.resolve(&mut format_args_wrapper, &mut errors)?
                    else {
                        return Ok(errors);
                    };
                    resolved = format::FormatItem::Spec(spec);
                    &resolved
                } else {
                    item
                }
            } else {
                item
            };
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

    Ok(errors)
}

// uucore's CanAsterisk and ArgumentLocation types are not re-exported, so modifier
// metadata must be retained separately from its parsed Spec.
struct FormatModifiers {
    prefix: String,
    width: String,
    precision: Option<String>,
    suffix: String,
}

impl FormatModifiers {
    fn resolve(
        &self,
        args: &mut format::FormatArguments<'_>,
        errors: &mut Vec<String>,
    ) -> Result<Option<format::Spec>, brush_core::Error> {
        let mut spec = self.prefix.clone();
        let ControlFlow::Continue(width) = resolve_modifier(&self.width, false, args, errors)?
        else {
            return Ok(None);
        };
        if let Some(width) = width {
            spec.push_str(&width);
        }
        if let Some(precision) = &self.precision {
            let ControlFlow::Continue(precision) = resolve_modifier(precision, true, args, errors)?
            else {
                return Ok(None);
            };
            if let Some(precision) = precision {
                spec.push('.');
                spec.push_str(&precision);
            }
        }
        spec.push_str(&self.suffix);
        format::Spec::parse(&mut spec.as_bytes())
            .map(Some)
            .map_err(|_| ErrorKind::PrintfInvalidUsage("invalid width or precision".into()).into())
    }
}

fn resolve_modifier(
    modifier: &str,
    precision: bool,
    args: &mut format::FormatArguments<'_>,
    errors: &mut Vec<String>,
) -> Result<ControlFlow<(), Option<String>>, brush_core::Error> {
    let (value, number) = if let Some(position) = modifier.strip_prefix('*') {
        // Use the formatter's argument-location and numeric parsing rules.
        let location_spec = format!("{position}b");
        let Ok(format::Spec::EscapedString { position }) =
            format::Spec::parse(&mut location_spec.as_bytes())
        else {
            return Err(ErrorKind::PrintfInvalidUsage("invalid argument position".into()).into());
        };
        let value = args.next_string(position);
        let arguments = [format::FormatArgument::Unparsed(value.to_os_string())];
        let Ok(format::Spec::EscapedString { position }) = format::Spec::parse(&mut &b"b"[..])
        else {
            unreachable!();
        };
        let number = if value.is_empty() {
            0
        } else {
            format::FormatArguments::new(&arguments).next_i64(position)
        };
        (value.to_string_lossy().into_owned(), i128::from(number))
    } else {
        if modifier.is_empty() {
            return Ok(ControlFlow::Continue(Some(String::new())));
        }
        (
            modifier.to_owned(),
            modifier.parse::<i128>().unwrap_or(i128::MAX),
        )
    };
    if number > i128::from(i32::MAX) || number < -i128::from(i32::MAX) - i128::from(precision) {
        if modifier.starts_with('*') {
            errors.push(format!("{value}: {}", modifier_error_message(false)));
            Ok(ControlFlow::Continue(None))
        } else {
            errors.push(modifier_error_message(true));
            Ok(ControlFlow::Break(()))
        }
    } else if (precision && number < 0) || (!precision && number == 0) {
        Ok(ControlFlow::Continue(None))
    } else {
        Ok(ControlFlow::Continue(Some(number.to_string())))
    }
}

fn modifier_error_message(literal: bool) -> String {
    #[cfg(unix)]
    {
        let errno = if literal {
            nix::errno::Errno::EOVERFLOW
        } else {
            nix::errno::Errno::ERANGE
        };
        uucore::error::strip_errno(&std::io::Error::from_raw_os_error(errno as i32))
    }
    #[cfg(not(unix))]
    {
        if literal {
            "Value too large for defined data type"
        } else {
            "Result too large"
        }
        .into()
    }
}

#[expect(
    clippy::string_slice,
    reason = "Slice boundaries delimit ASCII format syntax"
)]
fn prepare_modifiers(input: &str) -> (String, Vec<Option<FormatModifiers>>) {
    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut modifiers = vec![];
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' || (bytes[index] == b'%' && bytes.get(index + 1) == Some(&b'%')) {
            index += 2;
            continue;
        }
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        let prefix_start = index;
        let position_end = modifier_digits(bytes, index);
        if bytes.get(position_end) == Some(&b'$') {
            index = position_end + 1;
        }
        while bytes.get(index).is_some_and(|b| b"-+ #0'".contains(b)) {
            index += 1;
        }
        let width_start = index;
        index = modifier_end(bytes, index);
        let width_end = index;
        let precision = if bytes.get(index) == Some(&b'.') {
            index += 1;
            let start = index;
            index = modifier_end(bytes, index);
            Some(input[start..index].to_owned())
        } else {
            None
        };
        let suffix_start = index;
        while bytes.get(index).is_some_and(|b| b"hljztL".contains(b)) {
            index += 1;
        }
        if bytes
            .get(index)
            .is_some_and(|b| b"csdiuoxXfeEgGaAFG".contains(b))
        {
            index += 1;
            output.push_str(&input[copied..=start]);
            output.push_str(&input[prefix_start..width_start]);
            if width_end > width_start {
                output.push('1');
            }
            if precision.is_some() {
                output.push_str(".0");
            }
            output.push_str(&input[suffix_start..index]);
            copied = index;
            modifiers.push(Some(FormatModifiers {
                prefix: input[prefix_start..width_start].to_owned(),
                width: input[width_start..width_end].to_owned(),
                precision,
                suffix: input[suffix_start..index].to_owned(),
            }));
        } else {
            modifiers.push(None);
            index += 1;
        }
    }
    output.push_str(&input[copied..]);
    (output, modifiers)
}

fn modifier_digits(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    index
}

fn modifier_end(bytes: &[u8], index: usize) -> usize {
    if bytes.get(index) == Some(&b'*') {
        let end = modifier_digits(bytes, index + 1);
        if bytes.get(end) == Some(&b'$') {
            end + 1
        } else {
            index + 1
        }
    } else {
        modifier_digits(bytes, index)
    }
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

    #[test]
    fn test_overflowing_modifiers() -> Result<()> {
        for (fmt, args, expected, errors) in [
            (
                "[%*s]",
                vec!["99999999999", "x"],
                "[x]",
                vec!["99999999999"],
            ),
            (
                "[%.*s]",
                vec!["99999999999", "xyz"],
                "[xyz]",
                vec!["99999999999"],
            ),
            (
                "[%999999999999999999999999999999999999999s]",
                vec!["x"],
                "[",
                vec!["literal overflow"],
            ),
            (
                "[%5.*s]",
                vec!["99999999999", "x"],
                "[    x]",
                vec!["99999999999"],
            ),
            (
                "[%*.*s]",
                vec!["99999999999", "2", "xyz"],
                "[xy]",
                vec!["99999999999"],
            ),
            ("[%*s]", vec!["99999999999", ""], "[]", vec!["99999999999"]),
            (
                "[%*s]",
                vec!["99999999999", "x", "3", "y"],
                "[x][  y]",
                vec!["99999999999"],
            ),
            ("[%*.*s]", vec!["-5", "2", "xyz"], "[xy   ]", vec![]),
            ("[%*.*s]", vec!["5", "-1", "xyz"], "[  xyz]", vec![]),
            ("[%*s]", vec![], "[]", vec![]),
            (
                "[%*.2s]",
                vec!["-99999999999", "xyz"],
                "[xy]",
                vec!["-99999999999"],
            ),
            (
                "[%.*s]",
                vec!["-99999999999", "xyz"],
                "[xyz]",
                vec!["-99999999999"],
            ),
            (
                "[%.99999999999s]",
                vec!["xyz"],
                "[",
                vec!["literal overflow"],
            ),
        ] {
            let mut output = vec![];
            let actual_errors = format_via_uucore(fmt, args.into_iter(), &mut output)?;
            assert_eq!(String::from_utf8(output)?, expected, "{fmt}");
            let expected_errors: Vec<_> = errors
                .into_iter()
                .map(|value| {
                    if value == "literal overflow" {
                        modifier_error_message(true)
                    } else {
                        format!("{value}: {}", modifier_error_message(false))
                    }
                })
                .collect();
            assert_eq!(actual_errors, expected_errors, "{fmt}");
        }
        Ok(())
    }
}
