//! Async pipe reading utilities for Unix.

use std::io;
use std::os::unix::io::OwnedFd;

use tokio::net::unix::pipe;

pub(crate) struct AsyncPipeReader(pipe::Receiver);

impl AsyncPipeReader {
    pub(crate) fn new(reader: std::io::PipeReader) -> io::Result<Self> {
        Ok(Self(pipe::Receiver::from_file(std::fs::File::from(
            OwnedFd::from(reader),
        ))?))
    }

    pub(crate) async fn read_to_string(&mut self) -> io::Result<String> {
        use tokio::io::AsyncReadExt;
        let mut s = String::new();
        self.0.read_to_string(&mut s).await?;
        Ok(s)
    }
}

/// Creates an async pipe pair (reader, writer).
pub(crate) fn async_pipe() -> io::Result<(pipe::Receiver, pipe::Sender)> {
    let (reader, writer) = std::io::pipe()?;
    let receiver = pipe::Receiver::from_file(std::fs::File::from(OwnedFd::from(reader)))?;
    let sender = pipe::Sender::from_file(std::fs::File::from(OwnedFd::from(writer)))?;
    Ok((receiver, sender))
}

/// Converts an async pipe receiver back to a blocking file.
pub(crate) fn receiver_into_blocking(receiver: pipe::Receiver) -> io::Result<std::fs::File> {
    receiver.into_blocking_fd().map(std::fs::File::from)
}

/// Converts an async pipe sender back to a blocking file.
pub(crate) fn sender_into_blocking(sender: pipe::Sender) -> io::Result<std::fs::File> {
    sender.into_blocking_fd().map(std::fs::File::from)
}
