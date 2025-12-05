#![allow(clippy::missing_const_for_fn)]

use crate::error;
use std::path::PathBuf;

//
// Non-Unix implementation
//

pub(crate) fn get_user_home_dir(_username: &str) -> Option<PathBuf> {
    // std::env::home_dir() doesn't support getting home dir for arbitrary users
    // For now, we only support getting the current user's home dir
    None
}

pub(crate) fn get_current_user_home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

pub(crate) fn get_current_user_default_shell() -> Option<PathBuf> {
    None
}

pub(crate) fn is_root() -> bool {
    // TODO(windows): implement some version of this for Windows
    false
}

pub(crate) fn get_current_uid() -> Result<u32, error::Error> {
    Err(error::ErrorKind::NotSupportedOnThisPlatform("getting current uid").into())
}

pub(crate) fn get_current_gid() -> Result<u32, error::Error> {
    Err(error::ErrorKind::NotSupportedOnThisPlatform("getting current gid").into())
}

pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    Err(error::ErrorKind::NotSupportedOnThisPlatform("getting effective uid").into())
}

pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    Err(error::ErrorKind::NotSupportedOnThisPlatform("getting effective gid").into())
}

pub(crate) fn get_current_username() -> Result<String, error::Error> {
    let username = whoami::fallible::username()?;
    Ok(username)
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    // TODO(windows): implement some version of this for Windows
    Ok(vec![])
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // TODO(windows): implement some version of this for Windows
    Ok(vec![])
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    // TODO(windows): implement some version of this for Windows
    Ok(vec![])
}
