//! Filesystem utilities (stubs).

use crate::error;

pub(crate) trait MetadataExt {
    fn gid(&self) -> u32 {
        0
    }

    fn uid(&self) -> u32 {
        0
    }
}

impl MetadataExt for std::fs::Metadata {}

pub(crate) fn get_default_executable_search_paths() -> Vec<std::path::PathBuf> {
    vec![]
}

/// Returns the default paths where standard Unix utilities are typically installed.
/// This is a stub implementation that returns an empty vector.
pub fn get_default_standard_utils_paths() -> Vec<std::path::PathBuf> {
    vec![]
}

/// Opens a null file that will discard all I/O.
///
/// This is a stub implementation that returns an error.
pub fn open_null_file() -> Result<std::fs::File, error::Error> {
    Err(error::ErrorKind::NotSupportedOnThisPlatform("opening null file").into())
}

/// Gives the platform an opportunity to handle a special file path (e.g. `/dev/null`).
//
// This is a stub implementation that returns no result.
pub fn try_open_special_file(
    _path: &std::path::Path,
) -> Option<Result<std::fs::File, std::io::Error>> {
    None
}

/// Returns the path to the system-wide shell profile script.
///
/// Stub implementation that returns `None`.
pub fn get_system_profile_path() -> Option<&'static std::path::Path> {
    None
}

/// Returns the path to the system-wide shell rc script.
///
/// Stub implementation that returns `None`.
pub fn get_system_rc_path() -> Option<&'static std::path::Path> {
    None
}
