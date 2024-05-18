use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use crate::error;

/// Wait for jobs to terminate.
#[derive(Parser)]
pub(crate) struct WaitCommand {
    #[arg(short = 'f')]
    wait_for_terminate: bool,

    #[arg(short = 'n')]
    wait_for_first_or_next: bool,

    #[arg(short = 'p')]
    variable_to_receive_id: Option<String>,

    job_specs: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for WaitCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.wait_for_terminate {
            return error::unimp("wait -f");
        }
        if self.wait_for_first_or_next {
            return error::unimp("wait -n");
        }
        if self.variable_to_receive_id.is_some() {
            return error::unimp("wait -p");
        }
        if !self.job_specs.is_empty() {
            return error::unimp("wait with job specs");
        }

        context.shell.jobs.wait_all().await?;

        Ok(BuiltinExitCode::Success)
    }
}
