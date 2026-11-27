//! Polling utilities for Windows.

use std::os::windows::io::{AsRawHandle as _, RawHandle};

use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_PIPE, GetFileType};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;

use crate::openfiles::OpenFile;

pub use crate::sys::stubs::poll::poll_for_input;

/// Returns how many bytes can be read from an open file without blocking, as
/// `PeekNamedPipe` reports them, for the read end of an anonymous or named pipe.
///
/// Returns `None` for anything that isn't a pipe (regular files, consoles), for a pipe's
/// write end (which `PeekNamedPipe` can't read), for anything without a handle, and on
/// error.
///
/// # Arguments
///
/// * `file` - The open file to check.
pub fn pending_bytes(file: &OpenFile) -> Option<usize> {
    let handle: RawHandle = match file {
        OpenFile::Stdin(f) => f.as_raw_handle(),
        OpenFile::Stdout(f) => f.as_raw_handle(),
        OpenFile::Stderr(f) => f.as_raw_handle(),
        OpenFile::File(f) => f.as_raw_handle(),
        OpenFile::PipeReader(p) => p.as_raw_handle(),
        OpenFile::PipeWriter(p) => p.as_raw_handle(),
        OpenFile::Stream(_) => return None,
    };

    // SAFETY: `handle` stays open while `file` is borrowed, and `GetFileType` only queries it.
    if unsafe { GetFileType(handle) } != FILE_TYPE_PIPE {
        return None;
    }

    let mut available: u32 = 0;
    // SAFETY: `handle` stays open while `file` is borrowed. With no buffer (null, size 0)
    // `PeekNamedPipe` copies no data; it writes only the total available through the one
    // non-null out-pointer, which points to `available`.
    let peeked = unsafe {
        PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &raw mut available,
            std::ptr::null_mut(),
        )
    };
    if peeked == 0 {
        return None;
    }
    usize::try_from(available).ok()
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use std::io::{Read as _, Write as _};

    use super::*;

    /// A pipe's read end reports what's been written to it and not yet read; a regular file
    /// reports nothing.
    #[test]
    fn pending_bytes_counts_a_pipes_unread_bytes() -> std::io::Result<()> {
        let (reader, writer) = std::io::pipe()?;
        let (mut reader, mut writer) = (OpenFile::from(reader), OpenFile::from(writer));
        assert_eq!(pending_bytes(&reader), Some(0));

        writer.write_all(b"hello")?;
        assert_eq!(pending_bytes(&reader), Some(5));

        let mut buf = [0; 2];
        reader.read_exact(&mut buf)?;
        assert_eq!(pending_bytes(&reader), Some(3));

        let mut file = tempfile::tempfile()?;
        file.write_all(b"hello")?;
        assert_eq!(pending_bytes(&OpenFile::from(file)), None);
        Ok(())
    }
}
