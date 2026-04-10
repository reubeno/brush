//! Filesystem utilities

pub use super::platform::fs::*;

/// Extension trait for path-related filesystem operations.
pub trait PathExt {
    /// Returns true if the path exists and is readable by the current user.
    fn readable(&self) -> bool;
    /// Returns true if the path exists and is writable by the current user.
    fn writable(&self) -> bool;
    /// Returns true if the path exists and is executable by the current user.
    ///
    /// On Windows, this returns true if *either* the path itself is a file with
    /// a `PATHEXT` extension *or* appending some `PATHEXT` extension resolves
    /// to an existing file. To recover the actual on-disk path in the
    /// latter case, use [`resolve_executable`] which takes ownership
    /// and avoids copies on platforms where no resolution is needed.
    fn executable(&self) -> bool;

    /// Returns true if the path exists and is a block device.
    fn exists_and_is_block_device(&self) -> bool;
    /// Returns true if the path exists and is a character device.
    fn exists_and_is_char_device(&self) -> bool;
    /// Returns true if the path exists and is a FIFO (named pipe).
    fn exists_and_is_fifo(&self) -> bool;
    /// Returns true if the path exists and is a socket.
    fn exists_and_is_socket(&self) -> bool;
    /// Returns true if the path exists and has the setgid bit set.
    fn exists_and_is_setgid(&self) -> bool;
    /// Returns true if the path exists and has the setuid bit set.
    fn exists_and_is_setuid(&self) -> bool;
    /// Returns true if the path exists and has the sticky bit set.
    fn exists_and_is_sticky_bit(&self) -> bool;

    /// Returns the device ID and inode number for the path.
    fn get_device_and_inode(&self) -> Result<(u64, u64), crate::error::Error>;
}
