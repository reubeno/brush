//! Experimental filter facilities for shell extensions.
//!
//! This module provides a mechanism for intercepting and modifying shell operations
//! at key execution points. It is only available when the `experimental-filters` feature
//! is enabled.

use std::sync::Arc;
use tokio::sync::Mutex;

/// Trait that defines the input and output types for a filterable operation.
///
/// This trait only associates types; it does not include execution logic.
/// The actual execution is provided as a closure to [`do_with_filter`].
pub trait FilterableOp: Send {
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
    #[inline]
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
    #[inline]
    fn post_op(&mut self, output: O::Output) -> PostFilterResult<O> {
        PostFilterResult::Return(output)
    }
}

/// Default implementation of an operation filter that performs no filtering.
pub struct DefaultOpFilter<O: FilterableOp> {
    _marker: std::marker::PhantomData<O>,
}

impl<O: FilterableOp> Default for DefaultOpFilter<O> {
    fn default() -> Self {
        DefaultOpFilter {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<O: FilterableOp> OpFilter<O> for DefaultOpFilter<O> {}

/// Type alias for a boxed filter behind an async mutex.
pub type BoxedFilter<O> = Arc<Mutex<dyn OpFilter<O> + Send>>;

/// Macro for executing a filterable operation.
///
/// This macro handles all the boilerplate of checking for extensions and calling
/// pre/post filter methods, handling early returns.
///
/// # Arguments
///
/// * `$shell` - Expression yielding a reference to the Shell
/// * `$filter_method` - The base filter method name (e.g., `exec_simple_command_filter`)
/// * `$input_val` - The input value to pass to the pre filter method
/// * `|$input_ident|` - Binding name for the (possibly modified) input in body
/// * `$body` - The expression to execute
///
/// # Variants
///
/// - Default: Can `return` from enclosing function if pre filter returns `Return`
/// - `no_return:`: Captures output instead of returning (for intermediate filters)
///
/// # Example
///
/// ```ignore
/// crate::with_filter!(self.shell, expand_word_filter, word, |word| {
///     self.basic_expand_impl(word).await
/// })
/// ```
#[macro_export]
macro_rules! with_filter {
    // Variant that can early-return - exec_simple_command_filter
    ($shell:expr, exec_simple_command_filter, $input_val:expr, |$input_ident:ident| $body:expr) => {{
        let mut $input_ident = $input_val;

        // If we have extensions, apply pre filter
        if let Some(ext) = $shell.extensions() {
            match ext.pre_exec_simple_command($input_ident) {
                $crate::filter::PreFilterResult::Continue(new_input) => {
                    $input_ident = new_input;
                }
                $crate::filter::PreFilterResult::Return(output) => {
                    return output;
                }
            }
        }

        // Execute the body
        let output = $body;

        // If we have extensions, apply post filter
        if let Some(ext) = $shell.extensions() {
            match ext.post_exec_simple_command(output) {
                $crate::filter::PostFilterResult::Return(output) => output,
            }
        } else {
            output
        }
    }};

    // Variant that can early-return - exec_external_command_filter
    ($shell:expr, exec_external_command_filter, $input_val:expr, |$input_ident:ident| $body:expr) => {{
        let mut $input_ident = $input_val;

        // If we have extensions, apply pre filter
        if let Some(ext) = $shell.extensions() {
            match ext.pre_exec_external_command($input_ident) {
                $crate::filter::PreFilterResult::Continue(new_input) => {
                    $input_ident = new_input;
                }
                $crate::filter::PreFilterResult::Return(output) => {
                    return output;
                }
            }
        }

        // Execute the body
        let output = $body;

        // If we have extensions, apply post filter
        if let Some(ext) = $shell.extensions() {
            match ext.post_exec_external_command(output) {
                $crate::filter::PostFilterResult::Return(output) => output,
            }
        } else {
            output
        }
    }};

    // Variant that can early-return - expand_word_filter
    ($shell:expr, expand_word_filter, $input_val:expr, |$input_ident:ident| $body:expr) => {{
        let mut $input_ident = $input_val;

        // If we have extensions, apply pre filter
        if let Some(ext) = $shell.extensions() {
            match ext.pre_expand_word($input_ident) {
                $crate::filter::PreFilterResult::Continue(new_input) => {
                    $input_ident = new_input;
                }
                $crate::filter::PreFilterResult::Return(output) => {
                    return output;
                }
            }
        }

        // Execute the body
        let output = $body;

        // If we have extensions, apply post filter
        if let Some(ext) = $shell.extensions() {
            match ext.post_expand_word(output) {
                $crate::filter::PostFilterResult::Return(output) => output,
            }
        } else {
            output
        }
    }};

    // Variant that can early-return - source_script_filter
    ($shell:expr, source_script_filter, $input_val:expr, |$input_ident:ident| $body:expr) => {{
        let mut $input_ident = $input_val;

        // If we have extensions, apply pre filter
        if let Some(ext) = $shell.extensions() {
            match ext.pre_source_script($input_ident) {
                $crate::filter::PreFilterResult::Continue(new_input) => {
                    $input_ident = new_input;
                }
                $crate::filter::PreFilterResult::Return(output) => {
                    return output;
                }
            }
        }

        // Execute the body
        let output = $body;

        // If we have extensions, apply post filter
        if let Some(ext) = $shell.extensions() {
            match ext.post_source_script(output) {
                $crate::filter::PostFilterResult::Return(output) => output,
            }
        } else {
            output
        }
    }};

    // Variant without early-return - exec_external_command_filter
    (no_return: $shell:expr, exec_external_command_filter, $input_val:expr, |$input_ident:ident| $body:expr) => {{
        let $input_ident = $input_val;

        // If we have extensions, apply pre filter and potentially execute
        let output = if let Some(ext) = $shell.extensions() {
            match ext.pre_exec_external_command($input_ident) {
                $crate::filter::PreFilterResult::Continue($input_ident) => {
                    // Execute body with potentially modified input
                    $body
                }
                $crate::filter::PreFilterResult::Return(output) => {
                    // Filter provided output directly, skip body
                    output
                }
            }
        } else {
            // No extensions, execute body directly
            $body
        };

        // If we have extensions, apply post filter
        if let Some(ext) = $shell.extensions() {
            match ext.post_exec_external_command(output) {
                $crate::filter::PostFilterResult::Return(output) => output,
            }
        } else {
            output
        }
    }};
}

pub use with_filter;
