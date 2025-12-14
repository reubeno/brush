//! Experimental filter facilities for shell extensions.
//!
//! This module provides a mechanism for intercepting and modifying shell operations
//! at key execution points. It is only available when the `experimental-filters` feature
//! is enabled.

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

/// Macro for executing a filterable operation with optional filtering.
///
/// This macro handles all the boilerplate of checking for extensions, cloning them,
/// and calling pre/post filter methods. The macro yields a value that the caller
/// can use (e.g., assign to a variable or return).
///
/// # Arguments
///
/// * `$shell` - Expression yielding a reference to the Shell (extensions are extracted first)
/// * `$pre_method` - Method name to call for pre-filtering (e.g., `pre_expand_word`)
/// * `$post_method` - Method name to call for post-filtering (e.g., `post_expand_word`)
/// * `$input_val` - The input value to pass to the pre filter method
/// * `|$input_ident|` - Binding name for the (possibly modified) input in body
/// * `$body` - The expression to execute if filtering continues
///
/// # Example
///
/// ```ignore
/// crate::with_filter!(
///     self.shell,
///     pre_expand_word,
///     post_expand_word,
///     word,
///     |word| self.basic_expand_impl(word).await
/// )
/// ```
#[macro_export]
macro_rules! with_filter {
    ($shell:expr, $pre_method:ident, $post_method:ident, $input_val:expr, |$input_ident:ident| $body:expr) => {{
        // Extract extensions FIRST, before any potential move of input
        let __extensions = $shell.extensions().clone_for_subshell();

        // Now safe to move input_val
        #[allow(unused_mut, reason = "may be needed based on calling context")]
        let mut __input_temp = $input_val;

        // Apply pre-filter
        match __extensions.$pre_method(__input_temp) {
            $crate::filter::PreFilterResult::Continue(__filtered_input) => {
                // Bind the filtered input and execute the body
                #[allow(unused_mut, reason = "may be needed based on calling context")]
                let mut $input_ident = __filtered_input;
                let __output = $body;
                // Apply post-filter
                match __extensions.$post_method(__output) {
                    $crate::filter::PostFilterResult::Return(__final_output) => __final_output,
                }
            }
            $crate::filter::PreFilterResult::Return(__output) => __output,
        }
    }};
}

pub use with_filter;
