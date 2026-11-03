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

/// Runs `f`, which may block its thread, without holding up the shell's other tasks.
///
/// `f` may block on reading a pipe or a file; among the tasks it mustn't hold up is
/// possibly the one that writes what `f` is waiting to read. Yields `f`'s output, or
/// an error if `f` couldn't run to completion (e.g., it panicked).
///
/// # Arguments
///
/// * `f` - The function to run.
pub async fn run_blocking<T>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, error::Error>
where
    T: Send + Sync + 'static,
{
    crate::sys::tasks::spawn_blocking(async move { f() })
        .await
        .await
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_blocking_yields_the_output() -> Result<(), error::Error> {
        assert_eq!(run_blocking(|| 42).await?, 42);
        Ok(())
    }

    /// A blocking function mustn't keep the shell's other tasks from running, even on
    /// a runtime with a single thread: here it waits for a task spawned before it.
    #[tokio::test(flavor = "current_thread")]
    async fn run_blocking_lets_other_tasks_run() -> Result<(), error::Error> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _task = crate::sys::tasks::spawn(async move {
            let _ = sender.send(());
        });
        let signaled = run_blocking(move || {
            receiver
                .recv_timeout(std::time::Duration::from_secs(10))
                .is_ok()
        })
        .await?;
        assert!(signaled, "the other task should have run");
        Ok(())
    }
}
