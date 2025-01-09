use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, error, timing};

/// Report on usage time.
#[derive(Parser)]
pub(crate) struct TimesCommand {}

impl builtins::Command for TimesCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        let (self_user, self_system) = crate::sys::resource::get_self_user_and_system_time()?;
        writeln!(
            context.stdout(),
            "{} {}",
            timing::format_duration_non_posixly(&self_user),
            timing::format_duration_non_posixly(&self_system),
        )?;

        let (children_user, children_system) =
            crate::sys::resource::get_children_user_and_system_time()?;
        writeln!(
            context.stdout(),
            "{} {}",
            timing::format_duration_non_posixly(&children_user),
            timing::format_duration_non_posixly(&children_system),
        )?;

        Ok(builtins::ExitCode::Success)
    }
}
