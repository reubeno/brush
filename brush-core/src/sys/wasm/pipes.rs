//! In-memory pipes for WASM.
//!
//! WASM targets have no OS pipes, and the shell's execution there is
//! single-threaded and cooperative. These pipes buffer everything written to
//! them (without bound) until it's read. Once every read end is gone, writes
//! fail with [`std::io::ErrorKind::BrokenPipe`], as they would on an OS pipe,
//! so a writer whose reader has stopped (e.g., `yes` in `yes | head`) stops
//! too rather than filling memory. A read from an empty pipe never
//! blocks: once all writers are gone it reports end-of-file, and while a
//! writer remains it fails with [`std::io::ErrorKind::WouldBlock`], as a
//! nonblocking pipe would, since nothing else could run to fill the pipe while
//! the reader blocked. Callers that can yield to other tasks (see
//! [`super::async_pipe`]) instead wait for writers to finish.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::openfiles::{OpenFile, Stream};

#[derive(Default)]
struct PipeState {
    buffer: VecDeque<u8>,
    readers: usize,
    writers: usize,
}

/// Shared state of a pipe, referenced by both of its ends.
#[derive(Default)]
struct Pipe(Mutex<PipeState>);

impl Pipe {
    fn lock(&self) -> std::sync::MutexGuard<'_, PipeState> {
        // The state is never left inconsistent, so a poisoned lock is still usable.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The read end of an in-memory pipe.
pub(crate) struct PipeReader(Arc<Pipe>);

impl PipeReader {
    fn new(pipe: Arc<Pipe>) -> Self {
        pipe.lock().readers += 1;
        Self(pipe)
    }
}

impl Clone for PipeReader {
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        self.0.lock().readers -= 1;
    }
}

impl std::io::Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut state = self.0.lock();
        if state.buffer.is_empty() && state.writers > 0 && !buf.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "the pipe is empty, and nothing can write to it until this command finishes",
            ));
        }
        let count = buf.len().min(state.buffer.len());
        for (dest, src) in buf.iter_mut().zip(state.buffer.drain(..count)) {
            *dest = src;
        }
        drop(state);
        Ok(count)
    }
}

impl std::io::Write for PipeReader {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("cannot write to read end of pipe"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Stream for PipeReader {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(self.clone())
    }
}

impl From<PipeReader> for OpenFile {
    fn from(reader: PipeReader) -> Self {
        Self::Stream(Box::new(reader))
    }
}

/// The write end of an in-memory pipe.
pub(crate) struct PipeWriter(Arc<Pipe>);

impl PipeWriter {
    fn new(pipe: Arc<Pipe>) -> Self {
        pipe.lock().writers += 1;
        Self(pipe)
    }
}

impl Clone for PipeWriter {
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }
}

impl Drop for PipeWriter {
    fn drop(&mut self) {
        self.0.lock().writers -= 1;
    }
}

impl std::io::Read for PipeWriter {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("cannot read from write end of pipe"))
    }
}

impl std::io::Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut state = self.0.lock();
        if state.readers == 0 {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        state.buffer.extend(buf);
        drop(state);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Stream for PipeWriter {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(self.clone())
    }
}

impl From<PipeWriter> for OpenFile {
    fn from(writer: PipeWriter) -> Self {
        Self::Stream(Box::new(writer))
    }
}

/// Creates a new in-memory pipe, returning its read and write ends.
#[allow(
    clippy::unnecessary_wraps,
    reason = "matches the signature on other platforms"
)]
pub(crate) fn pipe() -> std::io::Result<(PipeReader, PipeWriter)> {
    let pipe = Arc::new(Pipe::default());
    Ok((PipeReader::new(pipe.clone()), PipeWriter::new(pipe)))
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use std::io::{Read as _, Write as _};

    use super::*;

    #[test]
    fn writes_succeed_while_a_reader_remains() -> std::io::Result<()> {
        let (reader, mut writer) = pipe()?;
        let mut other_reader = reader.clone();
        drop(reader);

        writer.write_all(b"hi")?;
        drop(writer);

        let mut read = String::new();
        other_reader.read_to_string(&mut read)?;
        assert_eq!(read, "hi");
        Ok(())
    }

    #[test]
    fn writes_fail_with_broken_pipe_once_every_reader_is_gone() -> std::io::Result<()> {
        let (reader, mut writer) = pipe()?;
        let other_reader = reader.clone();
        drop(reader);
        drop(other_reader);

        assert!(matches!(
            writer.write(b"hi"),
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe
        ));
        Ok(())
    }
}
