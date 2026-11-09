//! Exploratory byte channels with lazy Unix descriptor materialization.
//!
//! Capacity is approximate: 64 messages of at most 1024 bytes, plus a reader's
//! partial chunk. Clones share the partial chunk, so short reads cannot reorder
//! bytes. Synchronous readers/writers wait on a shared `Condvar` when the channel
//! would block (never busy-sleeping); they must still run on blocking tasks, not
//! runtime worker threads. Async channel operations yield instead; bridges use
//! flume's async operations and Tokio's nonblocking Unix pipes.
//!
//! Each end independently transitions Channel -> Bridged. Materializing both
//! ends deliberately keeps two OS pipes and the channel between them: there is
//! no unsafe attempt to cancel a bridge and splice buffered bytes into one pipe.
//! See SPIKE.md for the tradeoffs and cancellation limitations.

use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use crate::openfiles::{OpenFile, Stream};
use bytes::Bytes;

const CHUNK: usize = 1024;
/// Fallback wait to recheck state when a bridge (not `try_write`/drop) makes
/// progress, or on spurious wakeups.
const SYNC_WAIT: Duration = Duration::from_millis(10);

struct ChannelNotify {
    lock: Mutex<()>,
    cv: Condvar,
}

impl ChannelNotify {
    const fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            cv: Condvar::new(),
        }
    }

    fn notify_all(&self) {
        self.cv.notify_all();
    }

    fn wait_timeout(&self, timeout: Duration) {
        let guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = self
            .cv
            .wait_timeout(guard, timeout)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

struct ReaderState {
    receiver: Option<flume::Receiver<Bytes>>,
    pending: Bytes,
}

struct ReaderInner {
    state: Mutex<ReaderState>,
    native: OnceLock<std::io::PipeReader>,
    notify: Arc<ChannelNotify>,
}

struct WriterInner {
    sender: Mutex<Option<flume::Sender<Bytes>>>,
    native: OnceLock<std::io::PipeWriter>,
    notify: Arc<ChannelNotify>,
}

impl Drop for WriterInner {
    fn drop(&mut self) {
        // Wake sync readers blocked on the condvar so they observe flume
        // disconnect (EOF) when the last writer handle is dropped.
        self.notify.notify_all();
    }
}

/// A cloneable channel read end. Clones share a cursor, like dup'd descriptors.
#[derive(Clone)]
pub struct ChannelReader(Arc<ReaderInner>);

/// A cloneable channel write end.
#[derive(Clone)]
pub struct ChannelWriter(Arc<WriterInner>);

/// Creates an approximately 64KiB bounded, binary-safe byte pipe.
#[must_use]
pub fn channel_pipe() -> (ChannelReader, ChannelWriter) {
    let (sender, receiver) = flume::bounded(64);
    let notify = Arc::new(ChannelNotify::new());
    (
        ChannelReader(Arc::new(ReaderInner {
            state: Mutex::new(ReaderState {
                receiver: Some(receiver),
                pending: Bytes::new(),
            }),
            native: OnceLock::new(),
            notify: Arc::clone(&notify),
        })),
        ChannelWriter(Arc::new(WriterInner {
            sender: Mutex::new(Some(sender)),
            native: OnceLock::new(),
            notify,
        })),
    )
}

fn broken_pipe() -> io::Error {
    io::ErrorKind::BrokenPipe.into()
}
fn would_block() -> io::Error {
    io::ErrorKind::WouldBlock.into()
}

fn missing(message: &'static str) -> io::Error {
    io::Error::other(message)
}

impl ChannelReader {
    // The same mutex covers consumption and materialization. No receive future
    // survives unlocking it: a bridge can never steal chunks from a pending read.
    fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(mut reader) = self.0.native.get() {
            // This is the legacy sync path; async callers offload native I/O.
            drop(state);
            return reader.read(buf);
        }
        if state.pending.is_empty() {
            let receiver = state
                .receiver
                .as_ref()
                .ok_or_else(|| missing("channel reader missing receiver"))?;
            match receiver.try_recv() {
                Ok(chunk) => {
                    state.pending = chunk;
                    // Freed a flume slot; wake writers that may be blocked on Full.
                    drop(state);
                    self.0.notify.notify_all();
                    state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
                }
                Err(flume::TryRecvError::Empty) => return Err(would_block()),
                Err(flume::TryRecvError::Disconnected) => return Ok(0),
            }
        }
        let len = buf.len().min(state.pending.len());
        buf[..len].copy_from_slice(&state.pending.split_to(len));
        drop(state);
        Ok(len)
    }

    /// Reads without blocking the runtime worker in channel mode.
    pub async fn read_async(&self, buf: &mut [u8]) -> io::Result<usize> {
        // Once native, use the bounded runtime blocking pool for legacy handles.
        // The mode check must happen under the state lock, including try_read:
        // materialization could otherwise race the check and cause a blocking read.
        loop {
            let result = {
                let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
                if self.0.native.get().is_some() {
                    None
                } else if buf.is_empty() {
                    Some(Ok(0))
                } else {
                    if state.pending.is_empty() {
                        let receiver = state
                            .receiver
                            .as_ref()
                            .ok_or_else(|| missing("channel reader missing receiver"))?;
                        match receiver.try_recv() {
                            Ok(chunk) => {
                                state.pending = chunk;
                                drop(state);
                                self.0.notify.notify_all();
                                state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
                            }
                            Err(flume::TryRecvError::Empty) => {}
                            Err(flume::TryRecvError::Disconnected) => return Ok(0),
                        }
                    }
                    if state.pending.is_empty() {
                        Some(Err(would_block()))
                    } else {
                        let n = buf.len().min(state.pending.len());
                        buf[..n].copy_from_slice(&state.pending.split_to(n));
                        drop(state);
                        Some(Ok(n))
                    }
                }
            };
            match result {
                None => return crate::openfiles::blocking_read(self.clone().into(), buf).await,
                Some(Err(e)) if e.kind() == io::ErrorKind::WouldBlock => {
                    tokio::task::yield_now().await;
                }
                Some(result) => return result,
            }
        }
    }

    /// Lazily creates the consumer-facing Unix pipe and starts the sole channel receiver.
    ///
    /// # Errors
    ///
    /// Returns an error if no Tokio runtime with an I/O reactor is available, if
    /// pipe creation/materialization fails, or if internal channel state is invalid.
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    pub fn materialize(&self) -> io::Result<&std::io::PipeReader> {
        use std::os::fd::OwnedFd;
        use tokio::io::AsyncWriteExt;

        if let Some(native) = self.0.native.get() {
            return Ok(native);
        }

        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        if self.0.native.get().is_none() {
            // Fail before modifying state when no reactor is available.
            let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
            let (reader, writer) = crate::sys::pipes::pipe()?;
            let mut writer = tokio::net::unix::pipe::Sender::from_file(std::fs::File::from(
                OwnedFd::from(writer),
            ))?;
            let receiver = state
                .receiver
                .take()
                .ok_or_else(|| missing("channel reader missing receiver"))?;
            let pending = std::mem::take(&mut state.pending);
            self.0
                .native
                .set(reader)
                .map_err(|_| io::Error::other("already materialized"))?;
            drop(state);
            self.0.notify.notify_all();
            runtime.spawn(async move {
                if writer.write_all(&pending).await.is_err() {
                    return;
                }
                while let Ok(chunk) = receiver.recv_async().await {
                    if writer.write_all(&chunk).await.is_err() {
                        break;
                    }
                }
                // Drop writer for EOF, or receiver for BrokenPipe upstream.
            });
        } else {
            drop(state);
        }

        self.0
            .native
            .get()
            .ok_or_else(|| missing("native handle missing after materialization"))
    }
}

impl ChannelWriter {
    fn try_write(&self, buf: &[u8], allow_native: bool) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let sender = self.0.sender.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(mut writer) = self.0.native.get() {
            if !allow_native {
                return Err(io::ErrorKind::Unsupported.into());
            }
            drop(sender);
            return writer.write(buf);
        }
        let n = CHUNK.min(buf.len());
        let tx = sender
            .as_ref()
            .ok_or_else(|| missing("channel writer missing sender"))?
            .clone();
        drop(sender);
        match tx.try_send(Bytes::copy_from_slice(&buf[..n])) {
            Ok(()) => {
                self.0.notify.notify_all();
                Ok(n)
            }
            Err(flume::TrySendError::Full(_)) => Err(would_block()),
            Err(flume::TrySendError::Disconnected(_)) => {
                self.0.notify.notify_all();
                Err(broken_pipe())
            }
        }
    }

    /// Writes all bytes cooperatively in channel mode; native writes use the blocking pool.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the channel/pipe is disconnected or if a native
    /// blocking write fails.
    pub async fn write_all_async(&self, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            match self.try_write(buf, false) {
                Ok(n) => buf = &buf[n..],
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => tokio::task::yield_now().await,
                Err(e) if e.kind() == io::ErrorKind::Unsupported => {
                    return crate::openfiles::blocking_write(self.clone().into(), buf.to_vec())
                        .await;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Lazily creates the producer-facing Unix pipe. Its bridge owns the only sender.
    ///
    /// # Errors
    ///
    /// Returns an error if no Tokio runtime with an I/O reactor is available, if
    /// pipe creation/materialization fails, or if internal channel state is invalid.
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    pub fn materialize(&self) -> io::Result<&std::io::PipeWriter> {
        use std::os::fd::OwnedFd;
        use tokio::io::AsyncReadExt;

        if let Some(native) = self.0.native.get() {
            return Ok(native);
        }

        let mut sender_guard = self.0.sender.lock().unwrap_or_else(PoisonError::into_inner);
        if self.0.native.get().is_none() {
            let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
            let (reader, writer) = crate::sys::pipes::pipe()?;
            let mut reader = tokio::net::unix::pipe::Receiver::from_file(std::fs::File::from(
                OwnedFd::from(reader),
            ))?;
            let sender = sender_guard
                .take()
                .ok_or_else(|| missing("channel writer missing sender"))?;
            self.0
                .native
                .set(writer)
                .map_err(|_| io::Error::other("already materialized"))?;
            drop(sender_guard);
            self.0.notify.notify_all();
            runtime.spawn(async move {
                let mut buf = vec![0; CHUNK];
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if sender
                                .send_async(Bytes::copy_from_slice(&buf[..n]))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            });
        } else {
            drop(sender_guard);
        }

        self.0
            .native
            .get()
            .ok_or_else(|| missing("native handle missing after materialization"))
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.try_read(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.0.notify.wait_timeout(SYNC_WAIT);
                }
                result => return result,
            }
        }
    }
}
impl Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            match self.try_write(buf, true) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.0.notify.wait_timeout(SYNC_WAIT);
                }
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Write for ChannelReader {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Read for ChannelWriter {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

impl Stream for ChannelReader {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(self.clone())
    }
    fn read_async<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> futures::future::BoxFuture<'a, io::Result<usize>> {
        Box::pin(self.read_async(buf))
    }
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, crate::Error> {
        use std::os::fd::AsFd;
        Ok(self.materialize()?.as_fd().try_clone_to_owned()?)
    }
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, crate::Error> {
        use std::os::fd::AsFd;
        Ok(self.materialize()?.as_fd())
    }
}
impl Stream for ChannelWriter {
    fn clone_box(&self) -> Box<dyn Stream> {
        Box::new(self.clone())
    }
    fn write_all_async<'a>(
        &'a self,
        buf: &'a [u8],
    ) -> futures::future::BoxFuture<'a, io::Result<()>> {
        Box::pin(self.write_all_async(buf))
    }
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, crate::Error> {
        use std::os::fd::AsFd;
        Ok(self.materialize()?.as_fd().try_clone_to_owned()?)
    }
    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, crate::Error> {
        use std::os::fd::AsFd;
        Ok(self.materialize()?.as_fd())
    }
}
impl From<ChannelReader> for OpenFile {
    fn from(reader: ChannelReader) -> Self {
        Self::Stream(Box::new(reader))
    }
}
impl From<ChannelWriter> for OpenFile {
    fn from(writer: ChannelWriter) -> Self {
        Self::Stream(Box::new(writer))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

    use super::*;

    #[test]
    fn short_reads_share_cursor_and_preserve_binary_order() {
        let (mut reader, mut writer) = channel_pipe();
        let data: Vec<_> = (0..4096).map(|n| (n % 256) as u8).collect();
        writer.write_all(&data).unwrap();
        drop(writer);
        let mut prefix = [0; 10];
        reader.read_exact(&mut prefix).unwrap();
        let mut rest = Vec::new();
        reader.clone().read_to_end(&mut rest).unwrap();
        assert_eq!([prefix.as_slice(), rest.as_slice()].concat(), data);
    }

    #[test]
    fn eof_requires_last_writer_and_drains_queue() {
        let (mut reader, mut writer) = channel_pipe();
        let other = writer.clone();
        writer.write_all(b"queued").unwrap();
        drop(writer);
        assert!(matches!(reader.try_read(&mut [0; 6]), Ok(6)));
        assert_eq!(
            reader.try_read(&mut [0; 1]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(other);
        assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
    }

    #[test]
    fn last_reader_drop_is_broken_pipe() {
        let (reader, mut writer) = channel_pipe();
        let other = reader.clone();
        drop(reader);
        writer.write_all(b"ok").unwrap();
        drop(other);
        assert_eq!(
            writer.write(b"no").unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[tokio::test]
    async fn async_backpressure_on_single_worker() {
        let (reader, writer) = channel_pipe();
        let data: Vec<_> = (0..1024 * 1024).map(|n| (n % 251) as u8).collect();
        let expected = data.clone();
        let producer = tokio::spawn(async move {
            writer.write_all_async(&data).await.unwrap();
        });
        let mut actual = Vec::new();
        let mut buf = [0; 317];
        loop {
            let n = reader.read_async(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            actual.extend_from_slice(&buf[..n]);
        }
        producer.await.unwrap();
        assert_eq!(actual, expected);
    }

    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    mod native {
        use super::*;
        use std::os::fd::{AsFd, AsRawFd};

        #[tokio::test]
        async fn reader_materialization_preserves_partial_chunk_and_eof() {
            let (mut reader, mut writer) = channel_pipe();
            writer.write_all(b"prefix\0suffix").unwrap();
            let mut prefix = [0; 3];
            reader.read_exact(&mut prefix).unwrap();
            assert_eq!(&prefix, b"pre");
            let fd = reader
                .materialize()
                .unwrap()
                .as_fd()
                .try_clone_to_owned()
                .unwrap();
            // Repeated materialization reuses the descriptor, not another bridge.
            assert_eq!(
                reader.materialize().unwrap().as_raw_fd(),
                reader.materialize().unwrap().as_raw_fd()
            );
            writer.write_all_async(b"tail").await.unwrap();
            drop(writer);
            let bytes = tokio::task::spawn_blocking(move || {
                let mut file = std::fs::File::from(fd);
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).unwrap();
                bytes
            })
            .await
            .unwrap();
            assert_eq!(bytes, b"fix\0suffixtail");
        }

        #[tokio::test]
        async fn writer_materialization_preserves_channel_prefix_and_eof() {
            let (reader, mut writer) = channel_pipe();
            writer.write_all(b"prefix\0").unwrap();
            let fd = writer
                .materialize()
                .unwrap()
                .as_fd()
                .try_clone_to_owned()
                .unwrap();
            drop(writer);
            tokio::task::spawn_blocking(move || {
                std::fs::File::from(fd).write_all(b"native").unwrap();
            })
            .await
            .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 3];
            loop {
                let n = reader.read_async(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&buf[..n]);
            }
            assert_eq!(bytes, b"prefix\0native");
        }

        #[tokio::test]
        async fn both_materialized_retain_bridges_and_all_prefix_bytes() {
            let (mut reader, mut writer) = channel_pipe();
            writer.write_all(b"old-prefix\0").unwrap();
            let mut prefix = [0; 4];
            reader.read_exact(&mut prefix).unwrap();
            let read_fd = reader
                .materialize()
                .unwrap()
                .as_fd()
                .try_clone_to_owned()
                .unwrap();
            let write_fd = writer
                .materialize()
                .unwrap()
                .as_fd()
                .try_clone_to_owned()
                .unwrap();
            drop(reader);
            drop(writer);
            let producer = tokio::task::spawn_blocking(move || {
                std::fs::File::from(write_fd)
                    .write_all(&vec![42; 256 * 1024])
                    .unwrap();
            });
            let bytes = tokio::task::spawn_blocking(move || {
                let mut bytes = Vec::new();
                std::fs::File::from(read_fd)
                    .read_to_end(&mut bytes)
                    .unwrap();
                bytes
            })
            .await
            .unwrap();
            producer.await.unwrap();
            assert_eq!(&prefix, b"old-");
            assert_eq!(&bytes[..7], b"prefix\0");
            assert_eq!(&bytes[7..], vec![42; 256 * 1024]);
        }

        #[tokio::test]
        async fn openfile_materialization_and_surviving_clones_route_native() {
            let (reader, writer) = channel_pipe();
            let read_file: OpenFile = reader.into();
            let write_file: OpenFile = writer.into();
            let read_fd = read_file.clone().try_clone_to_owned().unwrap();
            let write_fd = write_file
                .try_borrow_as_fd()
                .unwrap()
                .try_clone_to_owned()
                .unwrap();
            drop(read_fd);
            drop(write_fd);
            write_file.write_all_async(b"through-clones").await.unwrap();
            drop(write_file);
            let mut actual = Vec::new();
            let mut buf = [0; 5];
            loop {
                let n = read_file.read_async(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                actual.extend_from_slice(&buf[..n]);
            }
            assert_eq!(actual, b"through-clones");
        }

        #[tokio::test]
        async fn closed_native_consumer_disconnects_channel_writers() {
            let (reader, writer) = channel_pipe();
            let fd = reader
                .materialize()
                .unwrap()
                .as_fd()
                .try_clone_to_owned()
                .unwrap();
            drop(fd);
            drop(reader);
            let error = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if let Err(e) = writer.write_all_async(&[CHUNK as u8; CHUNK]).await {
                        break e;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        }
    }
}
