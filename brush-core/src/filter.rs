//! Filter facilities

use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Trait that defines the input and output types for a filterable operation.
///
/// This trait only associates types; it does not include execution logic.
/// The actual execution is provided as a closure to [`do_with_filter`].
pub trait FilterableOp {
    /// The input type consumed by this operation.
    type Input;
    /// The output type produced by this operation.
    type Output;
}

/// Result of a pre-operation filter.
pub enum PreFilterResult<O: FilterableOp> {
    /// Indicates that the operation should be executed with the given
    /// (possibly-updated) inputs.
    Continue(O::Input),
    /// Indicates that the operation should not be executed, and in its
    /// place the given outputs should be returned.
    Return(O::Output),
}

/// Result of a post-operation filter.
pub enum PostFilterResult<O: FilterableOp> {
    /// Indicates that the given (possibly-updated) outputs should be
    /// yielded as the results of the operation.
    Return(O::Output),
}

/// Trait implemented by a filter that can be applied to an operation of the
/// given type.
pub trait OpFilter<O: FilterableOp>: Send {
    /// Called before an operation is executed, providing the intended inputs to
    /// the operation. The implementation must return a result indicating how
    /// execution should proceed.
    ///
    /// # Arguments
    ///
    /// * `input` - The inputs that would be provided to the operation if it is to be executed.
    fn pre_op(&mut self, input: O::Input) -> PreFilterResult<O> {
        PreFilterResult::Continue(input)
    }

    /// Called after an operation is executed, providing the outputs produced by
    /// the operation. The implementation must return a result indicating how
    /// to return results from the execution.
    ///
    /// # Arguments
    ///
    /// * `output` - The outputs produced by the operation.
    fn post_op(&mut self, output: O::Output) -> PostFilterResult<O> {
        PostFilterResult::Return(output)
    }
}

/// Executes an operation with a filter.
///
/// This is the slow path, marked as `#[cold]` to optimize for the common case
/// where no filter is present.
#[cold]
async fn do_with_filter_slow<O, F, Exec, Fut>(
    input: O::Input,
    filter: &Arc<Mutex<F>>,
    executor: Exec,
) -> O::Output
where
    O: FilterableOp,
    F: OpFilter<O> + ?Sized,
    Exec: FnOnce(O::Input) -> Fut,
    Fut: Future<Output = O::Output> + Send,
{
    let mut filter_guard = filter.lock().await;
    let pre_op_result = filter_guard.pre_op(input);
    drop(filter_guard);

    let output = match pre_op_result {
        PreFilterResult::Continue(input) => executor(input).await,
        PreFilterResult::Return(output) => output,
    };

    let mut filter_guard = filter.lock().await;
    match filter_guard.post_op(output) {
        PostFilterResult::Return(output) => output,
    }
}

/// Executes an operation with the provided inputs, applying a filter if present.
///
/// The filter can inspect and modify inputs before execution, and inspect and
/// modify outputs after execution. If no filter is provided, the executor is
/// called directly.
///
/// This function is marked `#[inline]` to ensure the compiler can optimize
/// the no-filter fast path when filters are not used.
///
/// # Arguments
///
/// * `input` - The inputs to the operation.
/// * `filter` - The optional filter to apply to the operation.
/// * `executor` - The function that performs the actual operation.
///
/// # Type Parameters
///
/// * `O` - The filterable operation type (defines Input/Output types).
/// * `F` - The filter type.
/// * `Exec` - The executor closure type.
/// * `Fut` - The future type returned by the executor.
#[inline]
pub async fn do_with_filter<O, F, Exec, Fut>(
    input: O::Input,
    filter: &Option<Arc<Mutex<F>>>,
    executor: Exec,
) -> O::Output
where
    O: FilterableOp,
    F: OpFilter<O> + ?Sized,
    Exec: FnOnce(O::Input) -> Fut,
    Fut: Future<Output = O::Output> + Send,
{
    match filter {
        None => executor(input).await,
        Some(filter) => do_with_filter_slow::<O, F, Exec, Fut>(input, filter, executor).await,
    }
}
