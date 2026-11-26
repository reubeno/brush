//! Tasks the shell runs concurrently with itself.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use crate::error;

tokio::task_local! {
    static ON_A_THREAD_OF_ITS_OWN: ();
}

/// Runs `future` as a task that has a thread to itself.
///
/// Such a task, as one a
/// [`TaskSpawner::spawn_blocking`](crate::extensions::TaskSpawner::spawn_blocking)
/// runs on a thread of its own, has its reads and writes on pipes block that thread
/// rather than wait for the pipe through the runtime, as there's nothing else on it
/// to give way to. (On a current-thread runtime, a thread other than the one driving
/// the runtime can't wait through the runtime anyway: only that one can drive it.)
///
/// A task spawner that runs `spawn_blocking`'s futures on threads of their own should
/// run them through this; one that runs them on the caller's thread must not.
///
/// # Arguments
///
/// * `future` - The future to run.
pub fn on_a_thread_of_its_own<F: Future>(future: F) -> impl Future<Output = F::Output> {
    ON_A_THREAD_OF_ITS_OWN.scope((), future)
}

/// Returns whether the current task runs on a thread of its own; see
/// [`on_a_thread_of_its_own`].
#[must_use]
pub fn runs_on_a_thread_of_its_own() -> bool {
    ON_A_THREAD_OF_ITS_OWN.try_with(|()| ()).is_ok()
}

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
    pub fn from_future(
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
