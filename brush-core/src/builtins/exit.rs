use clap::Parser;

use std::io::Write;

use crate::{builtins, commands};

/// Exit the shell.
#[derive(Parser)]
pub(crate) struct ExitCommand {
    /// The exit code to return.
    code: Option<i32>,
}

impl builtins::Command for ExitCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if !context.shell.jobs.jobs.is_empty() && context.shell.user_tried_exiting == 0 {
            writeln!(context.stdout(), "brush: You have suspended jobs.")?;

            context.shell.user_tried_exiting = 2; // get's decreased this input too so next input will be 1

            return Ok(builtins::ExitCode::Custom(1))
        }

        for job in &mut context.shell.jobs.jobs {
            job.kill(Some(nix::sys::signal::SIGHUP))?;
        }

        let code_8bit: u8;

        #[allow(clippy::cast_sign_loss)]
        if let Some(code_32bit) = &self.code {
            code_8bit = (code_32bit & 0xFF) as u8;
        } else {
            code_8bit = context.shell.last_exit_status;
        }

        Ok(builtins::ExitCode::ExitShell(code_8bit))
    }
}
