//! Unix setup for the shells the harness spawns: a session of their own, and no fds but stdio.

use nix::libc::{self, c_int};
use std::os::unix::process::CommandExt;

/// Makes `cmd`'s child start a new session, holding only its stdin, stdout and stderr.
///
/// The new session keeps bash from opening `/dev/tty` and getting tty-related signals. (std's own
/// `setsid` is unstable, so this uses nix's.) The fds matter because other test threads open fds
/// that aren't close-on-exec, such as the pty master expectrl gets from `posix_openpt` and `dup`.
/// A shell started meanwhile would inherit them, and bash sees inherited fds where brush doesn't,
/// so cases that use fds could differ.
pub(crate) fn isolate(cmd: &mut std::process::Command) {
    let fd_limit = fd_limit();

    // SAFETY: the closure runs in the child between fork and exec, where it must not allocate or
    // take locks. It only makes syscalls.
    unsafe {
        cmd.pre_exec(move || {
            let _ = nix::unistd::setsid();
            mark_fds_cloexec(fd_limit);
            Ok(())
        });
    }
}

/// Marks every fd above stderr close-on-exec. They're not closed outright because std's
/// exec-error pipe is one of them: closing it would turn a failed exec into a bare exit status 1.
///
/// Linux and macOS each have a fast path, `mark_fds_cloexec_fast`. Elsewhere, or if that fails,
/// this tries every fd below `fd_limit`.
fn mark_fds_cloexec(fd_limit: c_int) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if mark_fds_cloexec_fast() {
        return;
    }

    (3..fd_limit).for_each(set_cloexec);
}

fn set_cloexec(fd: c_int) {
    // SAFETY: F_SETFD only sets the fd's flags; on an fd that isn't open, it fails with EBADF.
    let _ = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
}

/// Linux's fast path: one `close_range` marks every fd from 3 up. Returns false if the kernel
/// can't (before 5.11).
#[cfg(target_os = "linux")]
fn mark_fds_cloexec_fast() -> bool {
    // SAFETY: close_range with this flag only sets flags on fds. (It's a raw syscall because
    // libc's wrapper needs glibc 2.34, and musl has none.)
    let ret = unsafe {
        libc::syscall(
            libc::SYS_close_range,
            3,
            libc::c_uint::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    };
    ret == 0
}

/// macOS's fast path: `proc_pidinfo` lists the open fds, and only those from 3 up get an `fcntl`,
/// a dozen calls rather than one per possible fd. Returns false if it couldn't get the whole list.
#[cfg(target_os = "macos")]
fn mark_fds_cloexec_fast() -> bool {
    // On the stack, since this runs after fork. A full compat run peaks at about 250 open fds.
    let mut fds = [libc::proc_fdinfo {
        proc_fd: 0,
        proc_fdtype: 0,
    }; 1024];
    let Ok(size) = c_int::try_from(size_of_val(&fds)) else {
        return false;
    };

    // SAFETY: proc_pidinfo writes at most `size` bytes into `fds`; it's a thin syscall wrapper.
    let written = unsafe {
        libc::proc_pidinfo(
            nix::unistd::getpid().as_raw(),
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            size,
        )
    };

    // Nothing written means it failed; a full buffer may have cut the list short.
    let count = usize::try_from(written).unwrap_or(0) / size_of::<libc::proc_fdinfo>();
    if count == 0 || count == fds.len() {
        return false;
    }

    fds.iter()
        .take(count)
        .filter(|info| info.proc_fd >= 3)
        .for_each(|info| set_cloexec(info.proc_fd));
    true
}

/// Returns a bound on the fds this process can have open, for `mark_fds_cloexec`'s loop. It's
/// read before the fork.
fn fd_limit() -> c_int {
    use nix::sys::resource::{Resource, getrlimit};

    // shortcut: caps the loop at 64K fds, since containers often allow ~1M; raise it if the
    // harness ever holds that many open.
    getrlimit(Resource::RLIMIT_NOFILE).map_or(1024, |(soft, _)| {
        c_int::try_from(soft).unwrap_or(c_int::MAX).min(1 << 16)
    })
}
