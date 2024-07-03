use std::str::FromStr;

use crate::{error, traps};

pub(crate) fn parse_numeric_signal(signal: i32) -> Result<traps::TrapSignal, error::Error> {
    Ok(traps::TrapSignal::Signal(
        nix::sys::signal::Signal::try_from(signal).map_err(|_| error::Error::InvalidSignal)?,
    ))
}

pub(crate) fn parse_os_signal_name(signal: &str) -> Result<traps::TrapSignal, error::Error> {
    Ok(traps::TrapSignal::Signal(
        nix::sys::signal::Signal::from_str(signal).map_err(|_| error::Error::InvalidSignal)?,
    ))
}

pub(crate) fn continue_process(pid: u32) -> Result<(), error::Error> {
    #[allow(clippy::cast_possible_wrap)]
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::SIGCONT,
    )
    .map_err(|_errno| error::Error::FailedToSendSignal)?;
    Ok(())
}

pub(crate) fn kill_process(pid: u32) -> Result<(), error::Error> {
    #[allow(clippy::cast_possible_wrap)]
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::SIGKILL,
    )
    .map_err(|_errno| error::Error::FailedToSendSignal)?;

    Ok(())
}

const SIGTSTP: std::os::raw::c_int = 20;

pub(crate) fn tstp_signal_listener() -> Result<tokio::signal::unix::Signal, error::Error> {
    let signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::from_raw(SIGTSTP))?;
    Ok(signal)
}

pub(crate) use tokio::signal::ctrl_c as await_ctrl_c;
