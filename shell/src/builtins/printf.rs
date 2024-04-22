use clap::Parser;
use std::io::Write;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    env, variables,
};

#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PrintfCommand {
    #[arg(short = 'v')]
    output_variable: Option<String>,

    format: String,

    /// Args.
    #[clap(allow_hyphen_values = true)]
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for PrintfCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
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
            return Ok(BuiltinExitCode::Custom(output.status.code().unwrap() as u8));
        }

        if let Some(variable_name) = &self.output_variable {
            context.shell.env.update_or_add(
                variable_name.as_str(),
                variables::ShellValueLiteral::Scalar(stdout),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        } else {
            write!(context.stdout(), "{stdout}")?;
            context.stdout().flush()?;
        }

        return Ok(BuiltinExitCode::Success);
    }
}
