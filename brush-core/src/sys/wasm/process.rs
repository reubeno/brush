//! Process management utilities: wasm targets can't start processes.

use crate::processes::{ChildProcess, LaunchRequest};

pub(crate) type ProcessId = i32;

/// Starts a process; not supported on this platform.
///
/// # Arguments
///
/// * `_command` - The process to start.
#[allow(
    clippy::needless_pass_by_value,
    reason = "matches the signature of the platforms that consume the request"
)]
pub(crate) fn spawn(_command: LaunchRequest) -> std::io::Result<ChildProcess> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "this platform can't start processes",
    ))
}

/// Replaces the current process with a new one; not supported on this platform.
///
/// # Arguments
///
/// * `_command` - The process to replace the current one with.
#[allow(
    clippy::needless_pass_by_value,
    reason = "matches the signature of the platforms that consume the request"
)]
pub(crate) fn exec(_command: LaunchRequest) -> std::io::Error {
    std::io::ErrorKind::Unsupported.into()
}
