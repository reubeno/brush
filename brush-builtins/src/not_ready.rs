//! Test input that isn't always ready, as an in-memory pipe on wasm isn't.

use std::collections::VecDeque;
use std::io::{Read, Write};

/// Input that has nothing to read on some calls (`None`), as an in-memory pipe on wasm
/// does while its writer is still running.
#[derive(Clone)]
pub(crate) struct NotReadyYet(VecDeque<Option<Vec<u8>>>);

impl NotReadyYet {
    pub(crate) fn file(
        reads: impl IntoIterator<Item = Option<&'static [u8]>>,
    ) -> brush_core::openfiles::OpenFile {
        let reads = reads.into_iter().map(|r| r.map(<[u8]>::to_vec)).collect();
        brush_core::openfiles::OpenFile::Stream(Box::new(Self(reads)))
    }
}

impl Read for NotReadyYet {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let reads = &mut self.0;
        match reads.pop_front() {
            // Out of reads: end of input.
            None => Ok(0),
            Some(None) => Err(std::io::ErrorKind::WouldBlock.into()),
            Some(Some(mut data)) => {
                let n = buf.len().min(data.len());
                buf[..n].copy_from_slice(&data[..n]);
                if n < data.len() {
                    reads.push_front(Some(data.split_off(n)));
                }
                Ok(n)
            }
        }
    }
}

impl Write for NotReadyYet {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::Unsupported.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl brush_core::openfiles::Stream for NotReadyYet {
    fn clone_box(&self) -> Box<dyn brush_core::openfiles::Stream> {
        Box::new(self.clone())
    }

    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, brush_core::Error> {
        Err(brush_core::ErrorKind::CannotConvertToNativeFd.into())
    }

    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, brush_core::Error> {
        Err(brush_core::ErrorKind::CannotConvertToNativeFd.into())
    }
}
