//! Filesystem utilities.

use crate::error;

impl crate::sys::fs::PathExt for std::path::Path {
    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        true
    }

    fn executable(&self) -> bool {
        true
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

pub(crate) trait MetadataExt {
    fn gid(&self) -> u32 {
        0
    }

    fn uid(&self) -> u32 {
        0
    }
}

impl MetadataExt for std::fs::Metadata {}

pub(crate) fn get_default_executable_search_paths() -> Vec<String> {
    vec![]
}

/// Returns the default paths where standard Unix utilities are typically installed.
/// This is a stub implementation that returns an empty vector.
pub fn get_default_standard_utils_paths() -> Vec<String> {
    vec![]
}

/// Opens a null file that will discard all I/O.
///
/// This is a stub implementation that returns an error.
pub fn open_null_file() -> Result<std::fs::File, error::Error> {
    Err(error::ErrorKind::NotSupportedOnThisPlatform("opening null file").into())
}
