use crate::{error, trace_categories};
use std::path::PathBuf;

// TODO: replace nix with <https://github.com/rust-lang/rust/issues/166252>
use nix::unistd::{self, Gid, Group, Uid, User};

pub(crate) fn is_root() -> bool {
    Uid::effective().is_root()
}

pub(crate) fn get_user_home_dir(username: &str) -> Option<PathBuf> {
    User::from_name(username).ok().flatten().map(|u| u.dir)
}

fn get_current_user() -> Option<User> {
    User::from_uid(unistd::getuid()).ok().flatten()
}

pub(crate) fn get_current_user_home_dir() -> Option<PathBuf> {
    get_current_user().map(|u| u.dir)
}

pub(crate) fn get_current_user_default_shell() -> Option<PathBuf> {
    get_current_user().map(|u| u.shell)
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_current_uid() -> Result<u32, error::Error> {
    Ok(unistd::getuid().as_raw())
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_current_gid() -> Result<u32, error::Error> {
    Ok(unistd::getgid().as_raw())
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    Ok(unistd::geteuid().as_raw())
}

#[expect(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    Ok(unistd::getegid().as_raw())
}

pub(crate) fn get_current_username() -> Result<String, error::Error> {
    let user = get_current_user().ok_or_else(|| error::ErrorKind::NoCurrentUser)?;
    Ok(user.name)
}

#[cfg(not(target_vendor = "apple"))]
pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    let groups = unistd::getgroups()?;
    Ok(groups.into_iter().map(Gid::as_raw).collect())
}

#[cfg(target_vendor = "apple")]
pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    let groups = get_current_user_groups()?;
    Ok(groups.into_iter().map(|g| g.gid.as_raw()).collect())
}

pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // TODO(#475): uzers::all_users() is available but unsafe; for now we just return the current
    // user. That's better than nothing.
    let user = get_current_username()?;
    Ok(vec![user])
}

pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    // TODO(#475): uzers::all_groups() is available but unsafe; for now we just return the current
    // user's groups. That's better than nothing.
    let groups = get_current_user_groups()?;
    let group_names = groups.into_iter().map(|g| g.name);
    Ok(group_names.collect())
}

#[cfg(not(target_vendor = "apple"))]
fn get_current_user_groups() -> Result<Vec<nix::unistd::Group>, error::Error> {
    let mut gids = unistd::getgroups().map_err(|_| error::ErrorKind::NoCurrentUser)?;
    let groups = gids
        .into_iter()
        .filter_map(|gid| nix::unistd::Group::from_gid(gid).ok().flatten())
        .collect();
    Ok(groups)
}

#[cfg(target_vendor = "apple")]
#[expect(clippy::unnecessary_wraps)]
fn get_current_user_groups() -> Result<Vec<nix::unistd::Group>, error::Error> {
    let groups = nix::unistd::Group::from_gid(unistd::getgid())
        .ok()
        .flatten()
        .into_iter()
        .collect();
    Ok(groups)
}
