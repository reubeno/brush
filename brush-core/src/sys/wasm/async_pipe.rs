//! Async pipe reading utilities for WASM.

use std::io::{self, Read};

use super::pipes::PipeReader;

pub(crate) struct AsyncPipeReader {
    inner: PipeReader,
}

impl AsyncPipeReader {
    #[allow(
        clippy::unnecessary_wraps,
        clippy::missing_const_for_fn,
        reason = "matches the signature on other platforms"
    )]
    pub(crate) fn new(reader: PipeReader) -> io::Result<Self> {
        Ok(Self { inner: reader })
    }

    /// Reads until all writers have closed the pipe, yielding to other tasks
    /// (e.g., the one writing to the pipe) whenever the pipe is empty.
    pub(crate) async fn read_to_string(&mut self) -> io::Result<String> {
        let mut bytes = vec![];
        loop {
            match self.inner.read_to_end(&mut bytes) {
                Ok(_) => break,
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    tokio::task::yield_now().await;
                }
                Err(err) => return Err(err),
            }
        }

        String::from_utf8(bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}
