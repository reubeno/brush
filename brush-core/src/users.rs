use crate::error;
use std::path::PathBuf;

#[cfg(unix)]
use uzers::os::unix::UserExt;

#[cfg(unix)]
pub(crate) fn is_root() -> bool {
    uzers::get_current_uid() == 0
}

#[cfg(unix)]
pub(crate) fn get_user_home_dir() -> Option<PathBuf> {
    if let Some(username) = uzers::get_current_username() {
        if let Some(user_info) = uzers::get_user_by_name(&username) {
            return Some(user_info.home_dir().to_path_buf());
        }
    }

    None
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    Ok(uzers::get_effective_uid())
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    Ok(uzers::get_effective_gid())
}

#[cfg(unix)]
pub(crate) fn get_current_username() -> Result<String, error::Error> {
    let username = uzers::get_current_username().ok_or_else(|| error::Error::NoCurrentUser)?;
    Ok(username.to_string_lossy().to_string())
}

#[allow(clippy::unnecessary_wraps)]
#[cfg(unix)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // TODO: implement this
    tracing::debug!("UNIMPLEMENTED: get_all_users");
    Ok(vec![])
}

#[allow(clippy::unnecessary_wraps)]
#[cfg(unix)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    // TODO: implement this
    tracing::debug!("UNIMPLEMENTED: get_all_groups");
    Ok(vec![])
}

#[cfg(windows)]
pub(crate) fn get_user_home_dir() -> Option<PathBuf> {
    homedir::get_my_home().unwrap_or_default()
}

#[cfg(windows)]
pub(crate) fn is_root() -> bool {
    // TODO: implement some version of this for Windows
    false
}

#[cfg(windows)]
pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    error::unimp("get effective uid")
}

#[cfg(windows)]
pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    error::unimp("get effective gid")
}

#[cfg(windows)]
pub(crate) fn get_current_username() -> Result<String, error::Error> {
    Ok(whoami::username())
}

#[cfg(windows)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // TODO: implement some version of this for Windows
    Ok(vec![])
}

#[cfg(windows)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    // TODO: implement some version of this for Windows
    Ok(vec![])
}
