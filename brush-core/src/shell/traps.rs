//! Trap handling for the shell.

use crate::{ExecutionResult, ProcessGroupPolicy, error};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Runs any exit steps for the shell.
    pub async fn on_exit(&mut self) -> Result<(), error::Error> {
        self.invoke_exit_trap_handler_if_registered().await?;

        Ok(())
    }

    async fn invoke_exit_trap_handler_if_registered(
        &mut self,
    ) -> Result<ExecutionResult, error::Error> {
        let Some(handler) = self
            .traps
            .get_handler(crate::traps::TrapSignal::Exit)
            .cloned()
        else {
            return Ok(ExecutionResult::success());
        };

        // TODO(traps): Confirm whether trap handlers should be executed in the same process group.
        let mut params = self.default_exec_params();
        params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

        let orig_last_exit_status = self.last_exit_status;

        self.enter_trap_handler(Some(&handler));

        let result = self
            .run_string(&handler.command, &handler.source_info, &params)
            .await;

        self.leave_trap_handler();
        self.last_exit_status = orig_last_exit_status;

        result
    }
}
