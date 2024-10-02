use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, expansion};

/// Format a string.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[arg(short = 'v')]
    output_variable: Option<String>,

    /// Format string.
    format: String,

    /// Arguments to the format string.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
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
        // Special-case common format string: "%s".
        if self.format == "%s" && self.args.len() == 1 {
            return Ok(self.args[0].clone());
        }

        self.evaluate_via_external_command(context)
    }

    #[expect(clippy::unwrap_in_result)]
    fn evaluate_via_external_command(
        &self,
        context: &commands::ExecutionContext<'_>,
    ) -> Result<String, crate::error::Error> {
        // TODO: Don't call external printf command.
        let mut cmd = std::process::Command::new("printf");
        cmd.env_clear();
        cmd.arg(&self.format);
        cmd.args(&self.args);

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
