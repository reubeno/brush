//! File descriptor utilities.

use std::os::fd::RawFd;

use crate::{error, openfiles};

#[cfg(target_os = "linux")]
const FD_DIR_PATH: &str = "/proc/self/fd";

#[cfg(target_os = "macos")]
const FD_DIR_PATH: &str = "/dev/fd";

/// Makes a best-effort attempt to iterate over all open file descriptors
/// for the current process.
///
/// If the platform does not support enumerating file descriptors, an empty iterator
/// is returned. This function will skip any file descriptors that cannot be opened.
pub fn try_iter_fds() -> impl Iterator<Item = (u32, openfiles::OpenFile)> {
    let mut opened_entries = vec![];

    if let Ok(fd_dir) = std::fs::read_dir(FD_DIR_PATH) {
        for entry in fd_dir.into_iter().flatten() {
            if let Ok(filename) = entry.file_name().into_string() {
                if let Ok(fd_num) = filename.parse::<RawFd>() {
                    // SAFETY:
                    // We are trying to open the file descriptor we found listed
                    // in the filesystem, but there's a risk that it's not the same one
                    // that we enumerated or that it's since been closed. For the purposes
                    // of this function, either of those outcomes are acceptable. We
                    // simply skip any fds that we can't open, and the function's purpose
                    // is to make a best-effort attempt to open all available fds.
                    if let Ok(file) = unsafe { open_file_by_fd(fd_num) } {
                        #[allow(clippy::cast_sign_loss)]
                        opened_entries.push((fd_num as u32, file));
                    }
                }
            }
        }
    }

    opened_entries.into_iter()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn iter_fds() -> Result<impl Iterator<Item = (u32, openfiles::OpenFile)>, error::Error> {
    Ok(std::iter::empty())
}

unsafe fn open_file_by_fd(fd: RawFd) -> Result<openfiles::OpenFile, error::Error> {
    // SAFETY: We are creating a BorrowedFd from a file descriptor. Callers typically
    // enumerate available file descriptors from procfs, devfs, or similar, but there's
    // still a risk that the fd has become invalid or closed since then -- or that this
    // function gets used incorrectly.
    let borrowed_fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
    let owned_fd = borrowed_fd.try_clone_to_owned()?;
    Ok(std::fs::File::from(owned_fd).into())
}
