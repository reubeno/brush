//! In-memory pipes for WASM.
//!
//! WASM targets have no OS pipes, and the shell's execution there is
//! single-threaded and cooperative. These pipes buffer everything written to
//! them (without bound) until it's read. A read from an empty pipe never
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
#[derive(Clone)]
pub(crate) struct PipeReader(Arc<Pipe>);

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
        self.0.lock().buffer.extend(buf);
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
    Ok((PipeReader(pipe.clone()), PipeWriter::new(pipe)))
}
