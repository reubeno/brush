//! Filesystem utilities for Windows.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::error;

// Selectively re-export items from stubs that we don't override.
pub(crate) use crate::sys::stubs::fs::MetadataExt;

/// Cached list of executable extensions from the PATHEXT environment variable.
static PATHEXT_EXTENSIONS: LazyLock<Vec<String>> = LazyLock::new(|| {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect()
});

/// Returns true if the path's extension is in the PATHEXT list.
fn has_executable_extension(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let dot_ext = format!(".{}", ext.to_string_lossy()).to_ascii_lowercase();
        PATHEXT_EXTENSIONS.contains(&dot_ext)
    } else {
        false
    }
}

impl crate::sys::fs::PathExt for Path {
    fn readable(&self) -> bool {
        self.exists()
    }

    fn writable(&self) -> bool {
        self.metadata().is_ok_and(|m| !m.permissions().readonly())
    }

    fn executable(&self) -> bool {
        // If the path as-is is an executable file, return true.
        if self.is_file() && has_executable_extension(self) {
            return true;
        }
        // Try appending each PATHEXT extension.
        for ext in PATHEXT_EXTENSIONS.iter() {
            let mut name = self.as_os_str().to_owned();
            name.push(ext);
            if Self::new(&name).is_file() {
                return true;
            }
        }
        false
    }

    fn exists_and_is_block_device(&self) -> bool {
        false
    }

    fn exists_and_is_char_device(&self) -> bool {
        false
    }

    fn exists_and_is_fifo(&self) -> bool {
        false
    }

    fn exists_and_is_socket(&self) -> bool {
        false
    }

    fn exists_and_is_setgid(&self) -> bool {
        false
    }

    fn exists_and_is_setuid(&self) -> bool {
        false
    }

    fn exists_and_is_sticky_bit(&self) -> bool {
        false
    }

    fn get_device_and_inode(&self) -> Result<(u64, u64), crate::error::Error> {
        Ok((0, 0))
    }
}

/// Splits a platform-specific PATH-like value into individual paths.
///
/// On Windows, this delegates to [`std::env::split_paths`].
pub fn split_paths<T: AsRef<OsStr> + ?Sized>(s: &T) -> std::env::SplitPaths<'_> {
    std::env::split_paths(s)
}

/// Opens a null file that will discard all I/O.
pub fn open_null_file() -> Result<std::fs::File, error::Error> {
    let f = std::fs::File::options()
        .read(true)
        .write(true)
        .open("NUL")?;
    Ok(f)
}

/// Gives the platform an opportunity to handle a special file path (e.g. `/dev/null`).
pub fn try_open_special_file(path: &Path) -> Option<Result<std::fs::File, std::io::Error>> {
    if path == Path::new("/dev/null") {
        Some(open_null_file().map_err(std::io::Error::other))
    } else {
        None
    }
}

/// Returns the default paths where executables are typically found on Windows.
pub(crate) fn get_default_executable_search_paths() -> Vec<PathBuf> {
    default_system_paths()
}

/// Returns the default paths where standard system utilities are found on Windows.
pub fn get_default_standard_utils_paths() -> Vec<PathBuf> {
    default_system_paths()
}

fn default_system_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(sysroot) = std::env::var("SystemRoot") {
        paths.push(PathBuf::from(&sysroot).join("system32"));
        paths.push(PathBuf::from(&sysroot));
        paths.push(PathBuf::from(&sysroot).join("System32").join("Wbem"));
        paths.push(
            PathBuf::from(&sysroot)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0"),
        );
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        paths.push(
            PathBuf::from(userprofile)
                .join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("WindowsApps"),
        );
    }
    paths
}

/// Returns the path to the system-wide shell profile script.
///
/// On Windows, no system profile is loaded by default.
pub const fn get_system_profile_path() -> Option<&'static Path> {
    None
}

/// Returns the path to the system-wide shell rc script.
///
/// On Windows, no system rc file is loaded by default.
pub const fn get_system_rc_path() -> Option<&'static Path> {
    None
}
