use std::io::Write;

use brush_core::{ExecutionResult, builtins, timing};

/// Report on usage time.
#[derive(Clone)]
pub(crate) struct TimesCommand {}

impl builtins::Command for TimesCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        bpaf::construct!(TimesCommand {})
    }

    fn about() -> &'static str {
        "Report on usage time."
    }

    fn synopsis() -> &'static str {
        ""
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (self_user, self_system) = brush_core::sys::resource::get_self_user_and_system_time()?;
        writeln!(
            context.stdout(),
            "{} {}",
            timing::format_duration_non_posixly(&self_user),
            timing::format_duration_non_posixly(&self_system),
        )?;

        let (children_user, children_system) =
            brush_core::sys::resource::get_children_user_and_system_time()?;
        writeln!(
            context.stdout(),
            "{} {}",
            timing::format_duration_non_posixly(&children_user),
            timing::format_duration_non_posixly(&children_system),
        )?;

        Ok(ExecutionResult::success())
    }
}
