//! The platform's side of the terminal and process groups.

/// Moves the process into a process group of its own, which it leads.
#[cfg(unix)]
pub(crate) fn lead_new_process_group() -> std::io::Result<()> {
    nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))?;
    Ok(())
}

/// Moves the process into a process group of its own; without process groups, there's none.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, reason = "matches other platforms")]
pub(crate) const fn lead_new_process_group() -> std::io::Result<()> {
    Ok(())
}

/// Has the process ignore SIGTTOU, which the kernel sends a process that takes the terminal's
/// foreground, or writes to the terminal with `tostop` set, from the background, and whose
/// default action stops it.
#[cfg(unix)]
pub(crate) fn ignore_sigttou() -> std::io::Result<()> {
    let ignore = nix::sys::signal::SigAction::new(
        nix::sys::signal::SigHandler::SigIgn,
        nix::sys::signal::SaFlags::empty(),
        nix::sys::signal::SigSet::empty(),
    );

    // SAFETY: no handler is installed, only the request that the OS ignore the signal, so
    // nothing runs in signal context.
    unsafe { nix::sys::signal::sigaction(nix::sys::signal::Signal::SIGTTOU, &ignore) }?;

    Ok(())
}

/// Has the process ignore SIGTTOU; without it, there's nothing to ignore.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, reason = "matches other platforms")]
pub(crate) const fn ignore_sigttou() -> std::io::Result<()> {
    Ok(())
}
