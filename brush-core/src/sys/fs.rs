//! Filesystem utilities

pub use super::platform::fs::*;

/// Extension trait for path-related filesystem operations.
pub trait PathExt: AsRef<std::path::Path> {
    /// Returns true if the path exists and is readable by the current user.
    fn readable(&self) -> bool;
    /// Returns true if the path exists and is writable by the current user.
    fn writable(&self) -> bool;
    /// Returns true if the path exists and is executable by the current user.
    fn executable(&self) -> bool;

    /// Resolves the path to an executable file if one exists.
    ///
    /// On Unix, this returns the path itself if it is executable. On Windows,
    /// this may append a `PATHEXT` extension to locate the actual file (so a
    /// lookup for `foo` can resolve to `foo.exe`). Callers that need to spawn
    /// the resulting path (e.g. `std::process::Command::new`) should prefer
    /// this over [`PathExt::executable`], since `CreateProcessW` does not
    /// auto-append extensions to fully-qualified paths.
    fn resolve_executable(&self) -> Option<std::path::PathBuf> {
        if self.executable() {
            Some(self.as_ref().to_path_buf())
        } else {
            None
        }
    }

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
