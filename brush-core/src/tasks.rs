//! Tasks the shell runs concurrently with itself.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::error;

/// A future that yields a task's output when it finishes.
type TaskFuture<T> = Pin<Box<dyn Future<Output = Result<T, error::Error>> + Send + Sync>>;

/// Tracks a task started on the shell's behalf. Awaiting it yields the task's
/// output, or an error if the task couldn't run to completion (e.g., it
/// panicked).
///
/// Dropping the handle doesn't stop the task.
pub struct TaskHandle<T> {
    future: TaskFuture<T>,
}

impl<T> TaskHandle<T> {
    /// Tracks a task through a future that yields its output when it finishes.
    ///
    /// # Arguments
    ///
    /// * `future` - The future to await for the task's output.
    pub(crate) fn from_future(
        future: impl Future<Output = Result<T, error::Error>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            future: Box::pin(future),
        }
    }
}

impl<T> Future for TaskHandle<T> {
    type Output = Result<T, error::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.future.as_mut().poll(cx)
    }
}
