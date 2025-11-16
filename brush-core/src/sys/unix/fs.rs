//! Filesystem utilities.

use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

use crate::error;

pub use std::os::unix::fs::MetadataExt;

const DEFAULT_EXECUTABLE_SEARCH_PATHS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

const DEFAULT_STANDARD_UTILS_PATHS: &[&str] =
    &["/bin", "/usr/bin", "/sbin", "/usr/sbin", "/etc", "/usr/etc"];

impl crate::sys::fs::PathExt for Path {
    fn readable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::R_OK).is_ok()
    }

    fn writable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::W_OK).is_ok()
    }

    fn executable(&self) -> bool {
        nix::unistd::access(self, nix::unistd::AccessFlags::X_OK).is_ok()
    }

    fn exists_and_is_block_device(&self) -> bool {
        try_get_file_type(self).is_some_and(|ft| ft.is_block_device())
    }

    fn exists_and_is_char_device(&self) -> bool {
        try_get_file_type(self).is_some_and(|ft| ft.is_char_device())
    }

    fn exists_and_is_fifo(&self) -> bool {
        try_get_file_type(self).is_some_and(|ft: std::fs::FileType| ft.is_fifo())
    }

    fn exists_and_is_socket(&self) -> bool {
        try_get_file_type(self).is_some_and(|ft| ft.is_socket())
    }

    fn exists_and_is_setgid(&self) -> bool {
        const S_ISGID: u32 = 0o2000;
        let file_mode = try_get_file_mode(self);
        file_mode.is_some_and(|mode| mode & S_ISGID != 0)
    }

    fn exists_and_is_setuid(&self) -> bool {
        const S_ISUID: u32 = 0o4000;
        let file_mode = try_get_file_mode(self);
        file_mode.is_some_and(|mode| mode & S_ISUID != 0)
    }

    fn exists_and_is_sticky_bit(&self) -> bool {
        const S_ISVTX: u32 = 0o1000;
        let file_mode = try_get_file_mode(self);
        file_mode.is_some_and(|mode| mode & S_ISVTX != 0)
    }

    fn get_device_and_inode(&self) -> Result<(u64, u64), crate::error::Error> {
        let metadata = self.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
}

fn try_get_file_type(path: &Path) -> Option<std::fs::FileType> {
    path.metadata().map(|metadata| metadata.file_type()).ok()
}

fn try_get_file_mode(path: &Path) -> Option<u32> {
    path.metadata().map(|metadata| metadata.mode()).ok()
}

pub(crate) fn get_default_executable_search_paths() -> Vec<String> {
    DEFAULT_EXECUTABLE_SEARCH_PATHS
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// Retrieves the platform-specific set of paths that should contain standard system
/// utilities. Used by `command -p`, for example.
pub fn get_default_standard_utils_paths() -> Vec<String> {
    //
    // Try to call confstr(_CS_PATH). If that fails, can't find a string value, or
    // finds an empty string, then we'll fall back to hard-coded defaults.
    //

    if let Ok(Some(cs_path)) = confstr_cs_path() {
        if !cs_path.is_empty() {
            return cs_path.split(':').map(|s| s.to_string()).collect();
        }
    }

    DEFAULT_STANDARD_UTILS_PATHS
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

fn confstr_cs_path() -> Result<Option<String>, std::io::Error> {
    let value = confstr(nix::libc::_CS_PATH)?;

    if let Some(value) = value {
        let value_str = value
            .into_string()
            .map_err(|_err| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid data"))?;
        Ok(Some(value_str))
    } else {
        Ok(None)
    }
}

/// A wrapper for [`nix::libc::confstr`]. Returns a value for the default PATH variable which
/// indicates where all the POSIX.2 standard utilities can be found.
///
/// N.B. We would strongly prefer to use a safe API exposed (in an idiomatic way) by nix
/// or similar. Until that exists, we accept the need to make the unsafe call directly.
fn confstr(name: nix::libc::c_int) -> Result<Option<std::ffi::OsString>, std::io::Error> {
    // SAFETY:
    // Calling `confstr` with a null pointer and size 0 is a documented way to query
    // the required size of the buffer to hold the value associated with `name`. It
    // should not end up causing any undefined behavior.
    let required_size = unsafe { nix::libc::confstr(name, std::ptr::null_mut(), 0) };

    // When confstr returns 0, it either means there's no value associated with _CS_PATH, or
    // _CS_PATH is considered invalid (and not present) on this platform. In both cases, we
    // treat it as a non-existent value and return None.
    if required_size == 0 {
        return Ok(None);
    }

    let mut buffer = Vec::<u8>::with_capacity(required_size);

    // SAFETY:
    // We are calling `confstr` with a valid pointer and size that we obtained from the
    // allocated buffer. Writing `c_char` (i8 or u8 depending on the platform) into
    // `Vec<u8>` is fine, as i8 and u8 have compatible representations, and Rust does
    // not support platforms where `c_char` is not 8-bit wide.
    let final_size =
        unsafe { nix::libc::confstr(name, buffer.as_mut_ptr().cast(), buffer.capacity()) };

    if final_size == 0 {
        return Err(std::io::Error::last_os_error());
    }

    // Per the docs on `confstr`, it *may* return a size larger than the provided buffer.
    // In our usage we wouldn't expect to see this, as we've first queried the required size.
    // However, we defensively check for this case and return an error if it happens.
    if final_size > buffer.capacity() {
        return Err(std::io::Error::other(
            "confstr needed more space than advertised",
        ));
    }

    // SAFETY:
    // We are trusting `confstr` to have written exactly `final_size` bytes into the buffer.
    // We have checked above that it didn't return a value *larger* than the capacity of
    // the buffer, and also checked for known error cases. Note that the returned length
    // should include the null terminator.
    unsafe { buffer.set_len(final_size) };

    // The last byte is a null terminator. We assert that it is.
    if !matches!(buffer.pop(), Some(0)) {
        return Err(std::io::Error::other(
            "confstr did not null-terminate the returned string",
        ));
    }

    Ok(Some(std::ffi::OsString::from_vec(buffer)))
}

/// Opens a null file that will discard all I/O.
pub fn open_null_file() -> Result<std::fs::File, error::Error> {
    let f = std::fs::File::options()
        .read(true)
        .write(true)
        .open("/dev/null")?;

    Ok(f)
}
