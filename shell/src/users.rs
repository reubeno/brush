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
pub(crate) fn get_current_username() -> Result<String, error::Error> {
    let username = uzers::get_current_username().ok_or_else(|| error::Error::NoCurrentUser)?;
    Ok(username.to_string_lossy().to_string())
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
pub(crate) fn get_current_username() -> Result<String, error::Error> {
    Ok(whoami::username())
}
