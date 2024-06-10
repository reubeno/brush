use clap::Parser;
use std::io::Write;

use crate::{builtin, commands, expansion};

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
impl builtin::Command for PrintfCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        // TODO: Don't call external printf command.
        let mut cmd = std::process::Command::new("printf");
        cmd.env_clear();
        cmd.arg(&self.format);
        cmd.args(&self.args);

        let output = cmd.output()?;

        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;

        write!(context.stderr(), "{stderr}")?;

        if !output.status.success() {
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_sign_loss)]
            return Ok(builtin::ExitCode::Custom(
                output.status.code().unwrap() as u8
            ));
        }

        if let Some(variable_name) = &self.output_variable {
            expansion::assign_to_named_parameter(context.shell, variable_name, stdout).await?;
        } else {
            write!(context.stdout(), "{stdout}")?;
            context.stdout().flush()?;
        }

        return Ok(builtin::ExitCode::Success);
    }
}
