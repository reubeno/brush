use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, escape, expansion};

/// Format a string.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[arg(short = 'v')]
    output_variable: Option<String>,

    /// Format string + arguments to the format string.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 1..)]
    format_and_args: Vec<String>,
}

impl builtins::Command for PrintfCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let result = self.evaluate(&context)?;

        if let Some(variable_name) = &self.output_variable {
            expansion::assign_to_named_parameter(context.shell, variable_name, result).await?;
        } else {
            write!(context.stdout(), "{result}")?;
            context.stdout().flush()?;
        }

        return Ok(builtins::ExitCode::Success);
    }
}

impl PrintfCommand {
    fn evaluate(
        &self,
        context: &commands::ExecutionContext<'_>,
    ) -> Result<String, crate::error::Error> {
        match self.format_and_args.as_slice() {
            // Special-case common format string: "%s".
            [fmt, arg] if fmt == "%s" => Ok(arg.clone()),
            // Special-case invocation of printf with %q-based format string from bash-completion.
            // It has hard-coded expectation of backslash-style escaping instead of quoting.
            [fmt, arg] if fmt == "%q" || fmt == "~%q" => {
                Ok(Self::evaluate_format_with_percent_q(None, arg))
            }
            [fmt, arg] if fmt == "~%q" => Ok(Self::evaluate_format_with_percent_q(Some("~"), arg)),
            // Fallback to external command.
            _ => self.evaluate_via_external_command(context),
        }
    }

    fn evaluate_format_with_percent_q(prefix: Option<&str>, arg: &str) -> String {
        let mut result = escape::quote_if_needed(arg, escape::QuoteMode::BackslashEscape);

        if let Some(prefix) = prefix {
            result.insert_str(0, prefix);
        }

        result
    }

    #[allow(clippy::unwrap_in_result)]
    fn evaluate_via_external_command(
        &self,
        context: &commands::ExecutionContext<'_>,
    ) -> Result<String, crate::error::Error> {
        // TODO: Don't call external printf command.
        let mut cmd = std::process::Command::new("printf");
        cmd.env_clear();
        cmd.args(&self.format_and_args);

        let output = cmd.output()?;

        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;

        write!(context.stderr(), "{stderr}")?;
        context.stderr().flush()?;

        if output.status.success() {
            Ok(stdout)
        } else {
            Err(crate::error::Error::PrintfFailure(
                output.status.code().unwrap(),
            ))
        }
    }
}
