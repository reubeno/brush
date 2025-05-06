use crate::error;
use std::path::PathBuf;

use uzers::os::unix::UserExt;

pub(crate) fn is_root() -> bool {
    uzers::get_current_uid() == 0
}

pub(crate) fn get_user_home_dir(username: &str) -> Option<PathBuf> {
    if let Some(user_info) = uzers::get_user_by_name(username) {
        return Some(user_info.home_dir().to_path_buf());
    }

    None
}

pub(crate) fn get_current_user_home_dir() -> Option<PathBuf> {
    if let Some(username) = uzers::get_current_username() {
        if let Some(user_info) = uzers::get_user_by_name(&username) {
            return Some(user_info.home_dir().to_path_buf());
        }
    }

    None
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    Ok(uzers::get_effective_uid())
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    Ok(uzers::get_effective_gid())
}

pub(crate) fn get_current_username() -> Result<String, error::Error> {
    let username = uzers::get_current_username().ok_or_else(|| error::Error::NoCurrentUser)?;
    Ok(username.to_string_lossy().to_string())
}

pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    let username = uzers::get_current_username().ok_or_else(|| error::Error::NoCurrentUser)?;
    let gid = uzers::get_current_gid();
    let groups = uzers::get_user_groups(&username, gid).unwrap_or_default();
    Ok(groups.into_iter().map(|g| g.gid()).collect())
}

#[cfg(target_os = "linux")]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    let users = uzers::all_users();
    let names = users
        .into_iter()
        .map(|u| u.name().to_string_lossy().to_string())
        .collect();

    Ok(names)
}

#[cfg(target_os = "macos")]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // We know through inspection that uzers::all_users() calls setpwent/getpwent/endpwent
    // in its implementation of all_users() on macOS. These functions are generally not
    // thread-safe on Unix-like platforms, but they *are* on macOS. Per documentation
    // from Apple, they internally store state in thread-local storage. We interpret this
    // to mean that, provided we aren't moved to a different thread and *also* that we're
    // not interrupted during our iteration. then we can "safely" call this unsafe
    // function.
    let users = unsafe { uzers::all_users() };
    let names = users
        .into_iter()
        .map(|u| u.name().to_string_lossy().to_string())
        .collect();

    Ok(names)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    Ok(vec![])
}

#[cfg(target_os = "linux")]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    let groups = uzers::all_groups();
    let names = groups
        .into_iter()
        .map(|g| g.name().to_string_lossy().to_string())
        .collect();

    Ok(names)
}

#[cfg(target_os = "macos")]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    // See block comment about safety in get_all_users().
    let groups = unsafe { uzers::all_groups() };
    let names = groups
        .into_iter()
        .map(|g| g.name().to_string_lossy().to_string())
        .collect();

    Ok(names)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    Ok(vec![])
}
