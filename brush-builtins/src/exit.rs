use brush_core::{ExecutionControlFlow, ExecutionResult, builtins};

/// Exit the shell.
#[derive(usage::Cli)]
#[usage(bin = "exit", unknown_flags = "error", args_override_self = false)]
pub(crate) struct ExitCommand {
    /// The exit code to return.
    // TODO(usage-migration): usage rejects `allow_hyphen_values` on a positional;
    // `allow_negative_numbers` covers the `-1`-style codes this builtin needs.
    #[usage(allow_negative_numbers)]
    code: Option<i64>,
}

impl builtins::Command for ExitCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        #[expect(clippy::cast_sign_loss)]
        let code_8bit = if let Some(code_32bit) = &self.code {
            (code_32bit & 0xFF) as u8
        } else {
            context.shell.last_exit_status()
        };

        let mut result = ExecutionResult::new(code_8bit);
        result.next_control_flow = ExecutionControlFlow::ExitShell;

        Ok(result)
    }
}

brush_core::impl_usage_parse!(ExitCommand);
