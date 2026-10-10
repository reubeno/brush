//! Tasks run on tokio's runtime.

use futures::FutureExt as _;

use crate::{error, tasks::TaskHandle};

/// Starts running a future concurrently with the caller.
///
/// # Arguments
///
/// * `future` - The future to run.
pub(crate) fn spawn<F>(future: F) -> TaskHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    TaskHandle::from_future(tokio::spawn(future).map(|r| r.map_err(join_error)))
}

/// Starts running a future that may block the thread it runs on (e.g., with
/// synchronous I/O) on a thread of its own, so it doesn't hold up the caller.
/// The returned future yields the task's handle once the caller may go on,
/// which here is right away.
///
/// This isn't an `async fn`, so the caller's future doesn't have to make room
/// for `future`.
///
/// # Arguments
///
/// * `future` - The future to run.
pub(crate) fn spawn_blocking<F>(future: F) -> impl Future<Output = TaskHandle<F::Output>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let handle =
        tokio::task::spawn_blocking(move || tokio::runtime::Handle::current().block_on(future));
    let task = TaskHandle::from_future(handle.map(|r| r.map_err(join_error)));
    std::future::ready(task)
}

fn join_error(err: tokio::task::JoinError) -> error::Error {
    error::ErrorKind::TaskError(Box::new(err)).into()
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use std::time::Duration;

    use super::*;

    /// How long a test task waits for the test to signal it.
    const TIMEOUT: Duration = Duration::from_secs(10);

    /// Returns whether `result` is the error for a task that didn't finish.
    fn is_task_error<T>(result: &Result<T, error::Error>) -> bool {
        result
            .as_ref()
            .is_err_and(|err| matches!(err.kind(), error::ErrorKind::TaskError(_)))
    }

    /// Unwinds the way a panic does, without printing a panic message.
    fn unwind() {
        std::panic::resume_unwind(Box::new("task unwound"));
    }

    #[tokio::test]
    async fn spawn_yields_the_tasks_output() -> Result<(), error::Error> {
        assert_eq!(spawn(async { 42 }).await?, 42);
        Ok(())
    }

    #[tokio::test]
    async fn spawn_yields_a_task_error_if_the_task_panics() {
        assert!(is_task_error(&spawn(async { unwind() }).await));
    }

    #[tokio::test]
    async fn spawn_blocking_yields_the_tasks_output() -> Result<(), error::Error> {
        assert_eq!(spawn_blocking(async { 42 }).await.await?, 42);
        Ok(())
    }

    #[tokio::test]
    async fn spawn_blocking_yields_a_task_error_if_the_task_panics() {
        assert!(is_task_error(
            &spawn_blocking(async { unwind() }).await.await
        ));
    }

    /// A task that blocks its thread mustn't keep the caller from going on: the
    /// task here blocks until the caller signals it.
    #[tokio::test]
    async fn spawn_blocking_does_not_hold_up_the_caller() -> Result<(), error::Error> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let task = spawn_blocking(async move { receiver.recv_timeout(TIMEOUT).is_ok() }).await;

        // If the task held up the caller, it has given up by now and dropped the
        // receiver, so this fails and so does the assertion below.
        let _ = sender.send(());
        assert!(task.await?, "the task should have been signaled");

        Ok(())
    }
}
