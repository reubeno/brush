//! The async runtime shells run on.

/// Returns a builder for the runtime a program's shells run on, as the `brush` binary builds
/// its own.
///
/// The runtime has worker threads where the platform has them, and every driver enabled (I/O,
/// time, processes, and signals). Its threads inherit the signal mask of the one that builds
/// it, so build it after any change to the process's signals that they're to share.
pub fn builder() -> tokio::runtime::Builder {
    #[cfg(any(unix, windows))]
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    #[cfg(not(any(unix, windows)))]
    let mut builder = tokio::runtime::Builder::new_current_thread();

    builder.enable_all();
    builder
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_runtime_runs_tasks() {
        let runtime = super::builder().build().unwrap();

        #[cfg(any(unix, windows))]
        assert_eq!(
            runtime.handle().runtime_flavor(),
            tokio::runtime::RuntimeFlavor::MultiThread
        );
        let sum = runtime.block_on(async { tokio::spawn(async { 1 + 1 }).await.unwrap() });
        assert_eq!(sum, 2);
    }
}
