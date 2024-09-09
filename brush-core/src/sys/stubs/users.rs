use crate::error;
use std::path::PathBuf;

pub(crate) fn get_user_home_dir(_username: &str) -> Option<PathBuf> {
    None
}

pub(crate) fn get_current_user_home_dir() -> Option<PathBuf> {
    None
}

pub(crate) fn is_root() -> bool {
    false
}

pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    error::unimp("get effective uid")
}

pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    error::unimp("get effective gid")
}

pub(crate) fn get_current_username() -> Result<String, error::Error> {
    error::unimp("get current username")
}

pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    Ok(vec![])
}

pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    Ok(vec![])
}
