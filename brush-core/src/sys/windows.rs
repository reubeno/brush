pub use crate::sys::stubs::async_pipe;
pub use crate::sys::stubs::commands;
pub(crate) mod env;
pub use crate::sys::stubs::fd;
pub(crate) mod fs;
pub use crate::sys::stubs::input;
pub(crate) mod network;
pub use crate::sys::stubs::poll;
pub use crate::sys::stubs::resource;

/// Signal processing utilities
pub mod signal {
    pub(crate) use crate::sys::stubs::signal::*;
    pub(crate) use tokio::signal::ctrl_c as await_ctrl_c;

    /// `STATUS_CONTROL_C_EXIT`, the status Windows gives a process that Ctrl-C ended.
    #[expect(clippy::cast_possible_wrap, reason = "an NTSTATUS is read as an i32")]
    const STATUS_CONTROL_C_EXIT: i32 = 0xC000_013A_u32 as i32;

    /// Returns whether a process exited because of Ctrl-C.
    ///
    /// # Arguments
    ///
    /// * `status` - The exit status of the process.
    pub(crate) fn exited_from_ctrl_c(status: std::process::ExitStatus) -> bool {
        status.code() == Some(STATUS_CONTROL_C_EXIT)
    }

    /// Returns the process exit code that tells a parent process this shell was stopped by
    /// Ctrl-C: `STATUS_CONTROL_C_EXIT`, the same status that a process Ctrl-C ended has.
    #[must_use]
    pub const fn interrupted_exit_code() -> Option<i32> {
        Some(STATUS_CONTROL_C_EXIT)
    }

    #[cfg(test)]
    mod tests {
        use std::os::windows::process::ExitStatusExt;

        use super::*;

        #[test]
        fn ctrl_c_exit_status_is_from_ctrl_c() {
            let status = std::process::ExitStatus::from_raw(0xC000_013A);
            assert!(exited_from_ctrl_c(status));
        }

        #[test]
        fn other_exit_statuses_are_not_from_ctrl_c() {
            for code in [0, 1, 130] {
                let status = std::process::ExitStatus::from_raw(code);
                assert!(!exited_from_ctrl_c(status), "code {code}");
            }
        }

        #[test]
        #[expect(clippy::cast_sign_loss, reason = "an NTSTATUS is read as a u32")]
        fn interrupted_exit_code_reads_as_ended_by_ctrl_c() {
            let read_back = interrupted_exit_code()
                .map(|code| exited_from_ctrl_c(std::process::ExitStatus::from_raw(code as u32)));
            assert_eq!(read_back, Some(true));
        }
    }
}

pub use crate::sys::stubs::terminal;
pub use crate::sys::tokio_process as process;
pub(crate) mod users;

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {}
