//! iOS: no user database is accessible from within the app sandbox; identity
//! comes from uid/gid syscalls and the process environment.

use crate::error;
use std::path::PathBuf;

pub(crate) const fn is_root() -> bool {
    false
}

pub(crate) fn get_user_home_dir(username: &str) -> Option<PathBuf> {
    // Only the current (sandboxed) user is resolvable.
    if get_current_username().is_ok_and(|current| current == username) {
        get_current_user_home_dir()
    } else {
        None
    }
}

pub(crate) fn get_current_user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub(crate) const fn get_current_user_default_shell() -> Option<PathBuf> {
    None
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_current_uid() -> Result<u32, error::Error> {
    // SAFETY: getuid is always successful and has no preconditions.
    Ok(unsafe { libc::getuid() })
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_current_gid() -> Result<u32, error::Error> {
    // SAFETY: getgid is always successful and has no preconditions.
    Ok(unsafe { libc::getgid() })
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    // SAFETY: geteuid is always successful and has no preconditions.
    Ok(unsafe { libc::geteuid() })
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    // SAFETY: getegid is always successful and has no preconditions.
    Ok(unsafe { libc::getegid() })
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_current_username() -> Result<String, error::Error> {
    Ok(std::env::var("USER").unwrap_or_else(|_| "mobile".to_owned()))
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    // SAFETY: getgid is always successful and has no preconditions.
    Ok(vec![unsafe { libc::getgid() }])
}

pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    Ok(vec![get_current_username()?])
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    Ok(vec!["mobile".to_owned()])
}
