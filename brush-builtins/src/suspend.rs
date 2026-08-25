use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, argmodel::ArgSpec, builtins};

/// Suspend the shell.
pub(crate) struct SuspendCommand {
    force: bool,
}

const ID_FORCE: &str = "force";

impl builtins::SpecCommand for SuspendCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[ArgSpec::flag(
                ID_FORCE,
                &['f'],
                &[],
                "Force suspend login shells.",
            )],
            positionals: &[],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            force: values.flag(ID_FORCE),
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
