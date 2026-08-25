use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Suspend the shell.
pub(crate) struct SuspendCommand {
    force: bool,
}

const ID_FORCE: &str = "force";

impl builtins::SpecCommand for SuspendCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_FORCE,
            &['f'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Force suspend login shells.",
        )
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            force: matches.flag(ID_FORCE),
        })
    }

    fn about() -> &'static str {
        "Suspend the shell."
    }

    fn synopsis() -> &'static str {
        "[-f]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if context.shell.options().login_shell && !self.force {
            writeln!(context.stderr(), "login shell cannot be suspended")?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        #[expect(clippy::cast_possible_wrap)]
        brush_core::sys::signal::kill_process(
            std::process::id() as i32,
            brush_core::traps::TrapSignal::Signal(nix::sys::signal::SIGSTOP),
        )?;

        Ok(ExecutionResult::success())
    }
}
