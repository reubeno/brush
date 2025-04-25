use crate::error;
use std::{ffi::CString, path::PathBuf};

use nix::{
    errno::Errno,
    unistd::{getgrouplist, Gid, Group, Uid, User},
};

pub(crate) fn is_root() -> bool {
    Uid::current().is_root()
}

pub(crate) fn get_user_home_dir(username: &str) -> Option<PathBuf> {
    User::from_name(username)
        .unwrap_or(None)
        .map(|user| user.dir)
}

pub(crate) fn get_current_user_home_dir() -> Option<PathBuf> {
    User::from_uid(Uid::effective())
        .unwrap_or(None)
        .map(|user| user.dir)
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_uid() -> Result<u32, error::Error> {
    Ok(Uid::effective().as_raw())
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_effective_gid() -> Result<u32, error::Error> {
    Ok(Gid::effective().as_raw())
}

pub(crate) fn get_current_username() -> Result<String, error::Error> {
    User::from_uid(Uid::effective())
        .map_err(error::Error::ErrnoError)?
        .map(|user| user.name)
        .ok_or(error::Error::NoCurrentUser)
}

pub(crate) fn get_user_group_ids() -> Result<Vec<u32>, error::Error> {
    // If this string somehow returned something with a null byte in it, the system shouldn't be
    // relied on anymore, it'd be completely messed up.
    #[allow(clippy::expect_used)]
    let username = CString::new(get_current_username()?).expect("Unexpected early null byte");
    let group_gid = get_effective_gid()?;
    Ok(getgrouplist(&username, group_gid.into())?
        .iter()
        .map(|gid| gid.as_raw())
        .collect())
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_users() -> Result<Vec<String>, error::Error> {
    // TODO: uzers::all_users() is available but unsafe
    tracing::debug!("UNIMPLEMENTED: get_all_users");
    Ok(vec![])
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_all_groups() -> Result<Vec<String>, error::Error> {
    let mut res = vec![];
    for group in nix::unistd::getgroups()? {
        res.push(match Group::from_gid(group)? {
            Some(real_group) => real_group.name,
            None => return Err(error::Error::ErrnoError(Errno::EINVAL)),
        });
    }
    Ok(res)
}
