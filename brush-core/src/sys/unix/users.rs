use crate::{error, trace_categories};
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

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_current_uid() -> Result<u32, error::Error> {
    Ok(uzers::get_current_uid())
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_current_gid() -> Result<u32, error::Error> {
    Ok(uzers::get_current_gid())
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    Ok(uzers::get_effective_uid())
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    Ok(uzers::get_effective_gid())
}

pub(crate) fn get_current_username() -> Result<String, error::Error> {
    let username = uzers::get_current_username().ok_or_else(|| error::ErrorKind::NoCurrentUser)?;
    Ok(username.to_string_lossy().to_string())
}

pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    let username = uzers::get_current_username().ok_or_else(|| error::ErrorKind::NoCurrentUser)?;
    let gid = uzers::get_current_gid();
    let groups = uzers::get_user_groups(&username, gid).unwrap_or_default();
    Ok(groups.into_iter().map(|g| g.gid()).collect())
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // TODO(#475): uzers::all_users() is available but unsafe
    tracing::debug!(target: trace_categories::UNIMPLEMENTED, "get_all_users");
    Ok(vec![])
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    // TODO(#475): uzers::all_groups() is available but unsafe
    tracing::debug!(target: trace_categories::UNIMPLEMENTED, "get_all_groups");
    Ok(vec![])
}
