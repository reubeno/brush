pub(crate) use super::platform::fs::*;

#[cfg(not(unix))]
pub(crate) use StubMetadataExt as MetadataExt;
#[cfg(unix)]
pub(crate) use std::os::unix::fs::MetadataExt;

pub(crate) trait PathExt {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn executable(&self) -> bool;

    fn exists_and_is_block_device(&self) -> bool;
    fn exists_and_is_char_device(&self) -> bool;
    fn exists_and_is_fifo(&self) -> bool;
    fn exists_and_is_socket(&self) -> bool;
    fn exists_and_is_setgid(&self) -> bool;
    fn exists_and_is_setuid(&self) -> bool;
    fn exists_and_is_sticky_bit(&self) -> bool;

    fn get_device_and_inode(&self) -> Result<(u64, u64), crate::error::Error>;
}
