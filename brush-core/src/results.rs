//! Encapsulation of execution results.

use crate::{error, processes};

/// Represents the result of executing a command or similar item.
#[derive(Default)]
pub struct ExecutionResult {
    /// The control flow transition to apply after execution.
    pub next_control_flow: ExecutionControlFlow,
    /// The exit code resulting from execution.
    pub exit_code: ExecutionExitCode,
}

impl ExecutionResult {
    /// Returns a new `ExecutionResult` with the given exit code.
    ///
    /// # Arguments
    ///
    /// * `exit_code` - The exit code of the command.
    pub fn new(exit_code: u8) -> Self {
        Self {
            exit_code: exit_code.into(),
            ..Self::default()
        }
    }

    /// Returns a new `ExecutionResult` reflecting a process that was stopped.
    pub fn stopped() -> Self {
        // TODO: Decide how to sort this out in a platform-independent way.
        const SIGTSTP: std::os::raw::c_int = 20;

        #[expect(clippy::cast_possible_truncation)]
        Self::new(128 + SIGTSTP as u8)
    }

    /// Returns a new `ExecutionResult` with an exit code of 0.
    pub const fn success() -> Self {
        Self {
            next_control_flow: ExecutionControlFlow::Normal,
            exit_code: ExecutionExitCode::Success,
        }
    }

    /// Returns a new `ExecutionResult` with a general error exit code.
    pub const fn general_error() -> Self {
        Self {
            next_control_flow: ExecutionControlFlow::Normal,
            exit_code: ExecutionExitCode::GeneralError,
        }
    }

    /// Returns whether the command was successful.
    pub const fn is_success(&self) -> bool {
        self.exit_code.is_success()
    }

    /// Returns whether the execution result indicates normal control flow.
    /// Returns `false` if there is any control flow transition requested.
    pub const fn is_normal_flow(&self) -> bool {
        matches!(self.next_control_flow, ExecutionControlFlow::Normal)
    }

    /// Returns whether the execution result indicates a loop break.
    pub const fn is_break(&self) -> bool {
        matches!(
            self.next_control_flow,
            ExecutionControlFlow::BreakLoop { .. }
        )
    }

    /// Returns whether the execution result indicates a loop continue.
    pub const fn is_continue(&self) -> bool {
        matches!(
            self.next_control_flow,
            ExecutionControlFlow::ContinueLoop { .. }
        )
    }

    /// Returns whether the execution result indicates an early return
    /// from a function or script, or an exit from the shell. Returns `false`
    /// otherwise, including loop breaks or continues.
    pub const fn is_return_or_exit(&self) -> bool {
        matches!(
            self.next_control_flow,
            ExecutionControlFlow::ReturnFromFunctionOrScript | ExecutionControlFlow::ExitShell
        )
    }
}

impl From<ExecutionExitCode> for ExecutionResult {
    fn from(exit_code: ExecutionExitCode) -> Self {
        Self {
            next_control_flow: ExecutionControlFlow::Normal,
            exit_code,
        }
    }
}

/// Represents an exit code from execution.
#[derive(Clone, Copy, Default)]
pub enum ExecutionExitCode {
    /// Indicates successful execution.
    #[default]
    Success,
    /// Indicates a general error.
    GeneralError,
    /// Indicates invalid usage.
    InvalidUsage,
    /// Cannot execute the command.
    CannotExecute,
    /// Indicates a command or similar item was not found.
    NotFound,
    /// Indicates execution was interrupted.
    Interrupted,
    /// Indicates unimplemented functionality was encountered.
    Unimplemented,
    /// A custom exit code.
    Custom(u8),
}

impl ExecutionExitCode {
    /// Returns whether the exit code indicates success.
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

impl From<u8> for ExecutionExitCode {
    fn from(code: u8) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::GeneralError,
            2 => Self::InvalidUsage,
            99 => Self::Unimplemented,
            126 => Self::CannotExecute,
            127 => Self::NotFound,
            130 => Self::Interrupted,
            code => Self::Custom(code),
        }
    }
}

impl From<ExecutionExitCode> for u8 {
    fn from(code: ExecutionExitCode) -> Self {
        Self::from(&code)
    }
}

impl From<&ExecutionExitCode> for u8 {
    fn from(code: &ExecutionExitCode) -> Self {
        match code {
            ExecutionExitCode::Success => 0,
            ExecutionExitCode::GeneralError => 1,
            ExecutionExitCode::InvalidUsage => 2,
            ExecutionExitCode::Unimplemented => 99,
            ExecutionExitCode::CannotExecute => 126,
            ExecutionExitCode::NotFound => 127,
            ExecutionExitCode::Interrupted => 130,
            ExecutionExitCode::Custom(code) => *code,
        }
    }
}

/// Represents a control flow transition to apply.
#[derive(Clone, Copy, Default)]
pub enum ExecutionControlFlow {
    /// Continue normal execution.
    #[default]
    Normal,
    /// Break out of an enclosing loop.
    BreakLoop {
        /// Identifies which level of nested loops to break out of. 0 indicates the innermost loop,
        /// 1 indicates the next outer loop, and so on.
        levels: usize,
    },
    /// Continue to the next iteration of an enclosing loop.
    ContinueLoop {
        /// Identifies which level of nested loops to continue. 0 indicates the innermost loop,
        /// 1 indicates the next outer loop, and so on.
        levels: usize,
    },
    /// Return from the current function or script.
    ReturnFromFunctionOrScript,
    /// Exit the shell.
    ExitShell,
}

impl ExecutionControlFlow {
    /// Attempts to decrement the loop levels for `BreakLoop` or `ContinueLoop`.
    /// If the levels reach zero, transitions to `Normal`. If the control flow is not
    /// a loop break or continue, no changes are made.
    #[must_use]
    pub const fn try_decrement_loop_levels(&self) -> Self {
        match self {
            Self::BreakLoop { levels: 0 } | Self::ContinueLoop { levels: 0 } => Self::Normal,
            Self::BreakLoop { levels } => Self::BreakLoop {
                levels: *levels - 1,
            },
            Self::ContinueLoop { levels } => Self::ContinueLoop {
                levels: *levels - 1,
            },
            control_flow => *control_flow,
        }
    }
}

/// Represents the result of spawning an execution; captures both execution
/// that immediately returns as well as execution that starts a process
/// asynchronously.
pub enum ExecutionSpawnResult {
    /// Indicates that the execution completed.
    Completed(ExecutionResult),
    /// Indicates that a process was started and had not yet completed.
    StartedProcess(processes::ChildProcess),
}

impl From<ExecutionResult> for ExecutionSpawnResult {
    fn from(result: ExecutionResult) -> Self {
        Self::Completed(result)
    }
}

impl ExecutionSpawnResult {
    /// Waits for the command to complete.
    ///
    /// # Arguments
    ///
    /// * `no_wait` - If true, do not wait for the command to complete; return immediately.
    pub async fn wait(self, no_wait: bool) -> Result<ExecutionWaitResult, error::Error> {
        match self {
            Self::StartedProcess(mut child) => {
                let process_wait_result = if !no_wait {
                    // Wait for the process to exit or for a relevant signal, whichever happens
                    // first.
                    child.wait().await?
                } else {
                    processes::ProcessWaitResult::Stopped
                };

                let wait_result = match process_wait_result {
                    processes::ProcessWaitResult::Completed(output) => {
                        ExecutionWaitResult::Completed(ExecutionResult::from(output))
                    }
                    processes::ProcessWaitResult::Stopped => ExecutionWaitResult::Stopped(child),
                };

                Ok(wait_result)
            }
            Self::Completed(result) => Ok(ExecutionWaitResult::Completed(result)),
        }
    }
}

/// Represents the result of waiting for an execution to complete.
pub enum ExecutionWaitResult {
    /// Indicates that the execution completed.
    Completed(ExecutionResult),
    /// Indicates that the execution was stopped.
    Stopped(processes::ChildProcess),
}
