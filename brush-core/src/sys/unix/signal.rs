use crate::{error, sys, traps};

pub(crate) fn continue_process(pid: sys::process::ProcessId) -> Result<(), error::Error> {
    #[allow(clippy::cast_possible_wrap)]
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::SIGCONT)
        .map_err(|_errno| error::Error::FailedToSendSignal)?;
    Ok(())
}

pub(crate) fn kill_process(
    pid: sys::process::ProcessId,
    signal: traps::TrapSignal,
) -> Result<(), error::Error> {
    let translated_signal = match signal {
        traps::TrapSignal::Signal(signal) => signal,
        traps::TrapSignal::Debug | traps::TrapSignal::Err | traps::TrapSignal::Exit => {
            return Err(error::Error::InvalidSignal(signal.to_string()));
        }
    };

    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), translated_signal)
        .map_err(|_errno| error::Error::FailedToSendSignal)?;

    Ok(())
}

pub(crate) fn lead_new_process_group() -> Result<(), error::Error> {
    nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))?;
    Ok(())
}

pub(crate) fn tstp_signal_listener() -> Result<tokio::signal::unix::Signal, error::Error> {
    let signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::from_raw(
        nix::libc::SIGTSTP,
    ))?;
    Ok(signal)
}

pub(crate) fn chld_signal_listener() -> Result<tokio::signal::unix::Signal, error::Error> {
    let signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child())?;
    Ok(signal)
}

#[allow(unused)]
pub(crate) use tokio::signal::ctrl_c as await_ctrl_c;

pub(crate) fn mask_sigttou() -> Result<(), error::Error> {
    let ignore = nix::sys::signal::SigAction::new(
        nix::sys::signal::SigHandler::SigIgn,
        nix::sys::signal::SaFlags::empty(),
        nix::sys::signal::SigSet::empty(),
    );
    unsafe { nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGTTOU, &ignore) }?;
    Ok(())
}

pub(crate) fn poll_for_stopped_children() -> Result<bool, error::Error> {
    let mut found_stopped = false;

    loop {
        let wait_status = waitid_all(
            nix::sys::wait::WaitPidFlag::WUNTRACED | nix::sys::wait::WaitPidFlag::WNOHANG,
        );
        match wait_status {
            Ok(nix::sys::wait::WaitStatus::Stopped(_stopped_pid, _signal)) => {
                found_stopped = true;
            }
            Ok(_) => break,
            Err(nix::errno::Errno::ECHILD) => break,
            Err(e) => return Err(e.into()),
        }
    }

    Ok(found_stopped)
}

#[cfg(not(target_os = "macos"))]
fn waitid_all(
    flags: nix::sys::wait::WaitPidFlag,
) -> Result<nix::sys::wait::WaitStatus, nix::errno::Errno> {
    nix::sys::wait::waitid(nix::sys::wait::Id::All, flags)
}

//
// N.B. These functions were mostly copied from nix::sys::wait (https://github.com/nix-rust/nix, MIT license)
// to enable use of the `waitid` call on macOS. Ideally nix would expose it on macOS and we would
// remove this code.
//

#[cfg(target_os = "macos")]
fn waitid_all(
    flags: nix::sys::wait::WaitPidFlag,
) -> Result<nix::sys::wait::WaitStatus, nix::errno::Errno> {
    let siginfo = unsafe {
        // Memory is zeroed rather than uninitialized, as not all platforms
        // initialize the memory in the StillAlive case
        let mut siginfo: nix::libc::siginfo_t = std::mem::zeroed();
        nix::errno::Errno::result(nix::libc::waitid(
            nix::libc::P_ALL,
            0,
            &mut siginfo,
            flags.bits(),
        ))?;
        siginfo
    };

    siginfo_to_wait_status(siginfo)
}

#[cfg(target_os = "macos")]
fn siginfo_to_wait_status(
    siginfo: nix::libc::siginfo_t,
) -> Result<nix::sys::wait::WaitStatus, nix::errno::Errno> {
    let si_pid = unsafe { siginfo.si_pid() };
    if si_pid == 0 {
        return Ok(nix::sys::wait::WaitStatus::StillAlive);
    }

    let pid = nix::unistd::Pid::from_raw(si_pid);
    let si_status = unsafe { siginfo.si_status() };

    let status = match siginfo.si_code {
        nix::libc::CLD_EXITED => nix::sys::wait::WaitStatus::Exited(pid, si_status),
        nix::libc::CLD_KILLED | nix::libc::CLD_DUMPED => nix::sys::wait::WaitStatus::Signaled(
            pid,
            nix::sys::signal::Signal::try_from(si_status)?,
            siginfo.si_code == nix::libc::CLD_DUMPED,
        ),
        nix::libc::CLD_STOPPED => {
            nix::sys::wait::WaitStatus::Stopped(pid, nix::sys::signal::Signal::try_from(si_status)?)
        }
        nix::libc::CLD_CONTINUED => nix::sys::wait::WaitStatus::Continued(pid),
        _ => return Err(nix::errno::Errno::EINVAL),
    };

    Ok(status)
}
