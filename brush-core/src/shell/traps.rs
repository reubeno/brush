//! Trap handling for the shell.

use crate::{
    ExecutionControlFlow, ExecutionParameters, ExecutionResult, ProcessGroupPolicy, error,
    traps::TrapSignal,
};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Runs any exit steps for the shell.
    ///
    /// This currently includes invoking the `EXIT` trap handler, if any. If the
    /// handler itself exits the shell (via the `exit` builtin), the returned
    /// value carries the exit status it requested, which per bash replaces the
    /// shell's exit status (also recorded as the last exit status).
    ///
    /// # Arguments
    ///
    /// * `params` - Execution parameters to use for the handler.
    pub async fn on_exit(
        &mut self,
        params: &ExecutionParameters,
    ) -> Result<Option<u8>, error::Error> {
        if self.traps.has_live_handler(TrapSignal::Exit) {
            let trap_result = self.invoke_trap_handler(TrapSignal::Exit, params).await?;

            if matches!(
                trap_result.next_control_flow,
                ExecutionControlFlow::ExitShell
            ) {
                let status: u8 = trap_result.exit_code.into();
                self.set_last_exit_status(status);
                return Ok(Some(status));
            }
        }

        Ok(None)
    }

    /// Runs the shell's exit steps (see [`Self::on_exit`]) as a best effort,
    /// folding any exit status requested by the EXIT trap handler into the
    /// given execution result. Used at the shell-lifetime ends that report a
    /// result: `-c` commands, scripts, `( )` subshells, and command
    /// substitutions. (Per bash, EXIT traps should also fire at the end of
    /// process substitutions, background jobs, and pipeline-element subshells;
    /// those spawn paths don't yet fold. TODO(traps): cover them, ideally via a
    /// single clone-execute-fold helper shared by all child-shell run sites.)
    ///
    /// # Arguments
    ///
    /// * `result` - The execution result to fold the requested status into.
    /// * `params` - Execution parameters to use for the handler.
    pub(crate) async fn on_exit_folding_into(
        &mut self,
        result: &mut ExecutionResult,
        params: &ExecutionParameters,
    ) {
        if let Ok(Some(trap_requested_status)) = self.on_exit(params).await {
            result.exit_code = trap_requested_status.into();
        }
    }

    /// Invokes the handler registered for `signal`, if any.
    ///
    /// Behavior varies by signal type:
    ///
    /// * **Per-signal recursion guard** — each trap guards against its own self-recursion, but
    ///   different traps *can* fire from within each other's handlers (matching bash semantics).
    ///   (One knowing divergence: bash re-fires the `RETURN` trap for a `return` executed within
    ///   the handler itself, looping forever; the guard makes brush terminate instead.)
    ///
    /// * **Inheritance** — handlers a child shell (e.g. a subshell) didn't inherit at creation
    ///   time are downgraded to display-only there and never execute. The analogous behavior for
    ///   shell functions is implemented via trap suspension scopes entered/exited by
    ///   [`enter_function`](crate::Shell::enter_function) and
    ///   [`leave_function`](crate::Shell::leave_function).
    ///
    /// * **`$?` preservation** — `last_exit_status` is saved before and restored after the handler
    ///   runs so the trap does not clobber the status that triggered it.
    ///
    /// # Arguments
    ///
    /// * `signal`: Signal to run handler for.
    ///
    /// * `params`: Execution parameters to use for handler.
    pub(crate) async fn invoke_trap_handler(
        &mut self,
        signal: TrapSignal,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // Per-signal self-recursion guard: don't re-enter a trap that is
        // already being handled. Different traps *can* fire from each
        // other's handlers (e.g. ERR inside EXIT, EXIT inside ERR).
        if self.call_stack().is_trap_signal_active(signal) {
            return Ok(ExecutionResult::success());
        }

        // Don't fire traps that have been explicitly suppressed (e.g. DEBUG
        // during programmable completion).
        if self.call_stack().is_trap_delivery_suppressed() {
            return Ok(ExecutionResult::success());
        }

        // N.B. Only live handlers are considered; those retained solely so
        // `trap -p` can display them (traps a child shell didn't inherit from
        // its parent) never execute.
        let Some(handler) = self.traps.live_handler(signal).cloned() else {
            return Ok(ExecutionResult::success());
        };

        let mut params = params.clone();
        params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

        // Preserve $? across trap handler execution so the handler doesn't
        // clobber the status that triggered it.
        let orig_last_exit_status = self.last_exit_status;

        // N.B. We use manual enter/leave rather than an RAII guard because a guard
        // would need to hold `&mut Shell`, preventing the mutable borrow required by
        // `run_string()`. This is safe because `result` is captured into a variable
        // (never early-returned with `?`), so `leave_trap_handler()` always runs.
        self.enter_trap_handler(signal, Some(&handler));

        let result = self
            .run_string(&handler.command, &handler.source_info, &params)
            .await;

        self.leave_trap_handler();
        self.last_exit_status = orig_last_exit_status;

        result
    }

    /// Fires any live `RETURN` trap, folding the outcome into the given execution
    /// result. Invoked when a shell function returns and when a sourced script
    /// finishes executing.
    ///
    /// The trap is skipped if the result is already an error or is unwinding
    /// past the function/script boundary (an `exit`, or a `break`/`continue`
    /// escaping a sourced script into the caller's loop). An `exit` executed
    /// within the handler supersedes the original result (matching bash, where
    /// it exits the shell).
    ///
    /// # Arguments
    ///
    /// * `result` - The execution result of the function or sourced script.
    /// * `params` - Execution parameters to use for the handler.
    pub(crate) async fn invoke_return_trap(
        &mut self,
        result: Result<ExecutionResult, error::Error>,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let Ok(body_result) = result else {
            return result;
        };

        if !matches!(
            body_result.next_control_flow,
            ExecutionControlFlow::Normal | ExecutionControlFlow::ReturnFromFunctionOrScript
        ) || !self.traps.has_live_handler(TrapSignal::Return)
        {
            return Ok(body_result);
        }

        let trap_result = self.invoke_trap_handler(TrapSignal::Return, params).await?;
        if matches!(
            trap_result.next_control_flow,
            ExecutionControlFlow::ExitShell
        ) {
            return Ok(trap_result);
        }

        Ok(body_result)
    }
}
