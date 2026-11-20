//! Tasks run on tokio's (single-threaded) runtime.

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
    TaskHandle::from_future(
        tokio::spawn(future)
            .map(|r| r.map_err(|err| error::ErrorKind::TaskError(Box::new(err)).into())),
    )
}

/// Starts running a future that may block the thread it runs on (e.g., with
/// synchronous I/O). With no other threads to run it on, it would hold up
/// whatever the caller does next, so the returned future yields the task's
/// handle only once the task has finished.
///
/// # Arguments
///
/// * `future` - The future to run.
pub(crate) fn spawn_blocking<F>(future: F) -> impl Future<Output = TaskHandle<F::Output>>
where
    F: Future + Send + 'static,
    F::Output: Send + Sync + 'static,
{
    spawn(future).map(|result| TaskHandle::from_future(std::future::ready(result)))
}
