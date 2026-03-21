use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, timing};

/// Report on usage time.
#[derive(Parser)]
pub(crate) struct TimesCommand {}

impl builtins::Command for TimesCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut output = Vec::new();

        let (self_user, self_system) = brush_core::sys::resource::get_self_user_and_system_time()?;
        writeln!(
            output,
            "{} {}",
            timing::format_duration_non_posixly(&self_user),
            timing::format_duration_non_posixly(&self_system),
        )?;

        let (children_user, children_system) =
            brush_core::sys::resource::get_children_user_and_system_time()?;
        writeln!(
            output,
            "{} {}",
            timing::format_duration_non_posixly(&children_user),
            timing::format_duration_non_posixly(&children_system),
        )?;

        if let Some(mut stdout) = context.stdout_async() {
            stdout.write_all(&output).await?;
            stdout.flush().await?;
        } else {
            context.stdout().write_all(&output)?;
            context.stdout().flush()?;
        }

        Ok(ExecutionResult::success())
    }
}
