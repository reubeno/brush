//! Async standard streams for wasm, where tokio has no `io-std`.
//!
//! Each operation completes synchronously on the blocking std stream.

use std::io::{self, Read as _, Write as _};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Async wrapper for standard input.
pub struct Stdin(io::Stdin);

/// Async wrapper for standard output.
pub struct Stdout(io::Stdout);

/// Async wrapper for standard error.
pub struct Stderr(io::Stderr);

/// Returns the process's standard input.
pub fn stdin() -> Stdin {
    Stdin(io::stdin())
}

/// Returns the process's standard output.
pub fn stdout() -> Stdout {
    Stdout(io::stdout())
}

/// Returns the process's standard error.
pub fn stderr() -> Stderr {
    Stderr(io::stderr())
}

impl AsyncRead for Stdin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let n = self.0.read(buf.initialize_unfilled())?;
        buf.advance(n);
        Poll::Ready(Ok(()))
    }
}

macro_rules! impl_async_write {
    ($ty:ty) => {
        impl AsyncWrite for $ty {
            fn poll_write(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<io::Result<usize>> {
                Poll::Ready(self.0.write(buf))
            }

            fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                Poll::Ready(self.0.flush())
            }

            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
                self.poll_flush(cx)
            }
        }
    };
}

impl_async_write!(Stdout);
impl_async_write!(Stderr);
