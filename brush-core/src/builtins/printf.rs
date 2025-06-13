use clap::Parser;
use std::{io::Write, ops::ControlFlow};
use uucore::format;

use crate::{builtins, commands, error, escape, expansion};

/// Format a string.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[arg(short = 'v')]
    output_variable: Option<String>,

    /// Format string + arguments to the format string.
    #[arg(trailing_var_arg = true, required = true, allow_hyphen_values = true)]
    format_and_args: Vec<String>,
}

impl builtins::Command for PrintfCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        if let Some(variable_name) = &self.output_variable {
            // Format to a u8 vector.
            let mut result: Vec<u8> = vec![];
            format(self.format_and_args.as_slice(), &mut result)?;

            // Convert to a string.
            let result_str = String::from_utf8(result)
                .map_err(|_| error::Error::PrintfInvalidUsage("invalid UTF-8 output".into()))?;

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

        Ok(builtins::ExitCode::Success)
    }
}

fn format(format_and_args: &[String], writer: impl Write) -> Result<(), error::Error> {
    match format_and_args {
        // Special-case invocation of printf with %q-based format string from bash-completion.
        // It has hard-coded expectation of backslash-style escaping instead of quoting.
        [fmt, arg] if fmt == "%q" => format_special_case_for_percent_q(None, arg, writer),
        [fmt, arg] if fmt == "~%q" => format_special_case_for_percent_q(Some("~"), arg, writer),
        // Handle format string with arguments using uucore
        [fmt, args @ ..] => format_via_uucore(fmt, args.iter(), writer),
        // Handle case with no format string (we shouldn't be able to get here since clap will
        // fail parsing when the format string is missing)
        [] => Err(error::Error::PrintfInvalidUsage("missing operand".into())),
    }
}

fn format_special_case_for_percent_q(
    prefix: Option<&str>,
    arg: &str,
    mut writer: impl Write,
) -> Result<(), error::Error> {
    let mut result = escape::quote_if_needed(arg, escape::QuoteMode::BackslashEscape).to_string();

    if let Some(prefix) = prefix {
        result.insert_str(0, prefix);
    }

    write!(writer, "{result}")?;

    Ok(())
}

fn format_via_uucore(
    format_string: &str,
    args: impl Iterator<Item = impl AsRef<str>>,
    mut writer: impl Write,
) -> Result<(), error::Error> {
    // Convert string arguments to FormatArgument::Unparsed
    let format_args: Vec<_> = args
        .map(|s| format::FormatArgument::Unparsed(s.as_ref().to_string()))
        .collect();

    // Parse format string once.
    let format_items = parse_format_string(format_string)?;

    // Wrap the format arguments.
    let mut format_args_wrapper = format::FormatArguments::new(&format_args);

    // Keep going until we've exhausted all format arguments. Also make sure to run at least once
    // even if there's no format arguments.
    while format_args.is_empty() || !format_args_wrapper.is_exhausted() {
        // Process all format items, in order. We'll bail when we're told to stop.
        for item in &format_items {
            let control_flow = item
                .write(&mut writer, &mut format_args_wrapper)
                .map_err(|e| {
                    error::Error::PrintfInvalidUsage(format!("printf formatting error: {e}"))
                })?;

            if control_flow == ControlFlow::Break(()) {
                break;
            }
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

fn parse_format_string(
    format_string: &str,
) -> Result<Vec<format::FormatItem<format::EscapedChar>>, error::Error> {
    let format_items: Result<Vec<_>, _> =
        format::parse_spec_and_escape(format_string.as_bytes()).collect();

    // Observe any errors we encountered along the way.
    let format_items = format_items
        .map_err(|e| error::Error::PrintfInvalidUsage(format!("printf parsing error: {e}")))?;

    Ok(format_items)
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn sprintf_via_uucore(
        format_string: &str,
        args: impl Iterator<Item = impl AsRef<str>>,
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
