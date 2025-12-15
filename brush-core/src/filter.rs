//! Experimental filter facilities for shell extensions.
//!
//! This module provides a mechanism for intercepting and modifying shell operations
//! at key execution points. It is only available when the `experimental-filters` feature
//! is enabled.

/// Trait that defines the input and output types for a filterable operation.
///
/// This trait only associates types; it does not include execution logic.
/// The actual execution is provided as a closure to [`with_filter`].
pub trait FilterableOp: Send {
    /// The input type consumed by this operation.
    type Input;
    /// The output type produced by this operation.
    type Output;
}

/// Result of a pre-operation filter.
#[derive(Debug)]
pub enum PreFilterResult<O: FilterableOp> {
    /// Indicates that the operation should be executed with the given
    /// (possibly-updated) inputs.
    Continue(O::Input),
    /// Indicates that the operation should not be executed, and in its
    /// place the given outputs should be returned.
    Return(O::Output),
}

/// Result of a post-operation filter.
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::{ExpandWordOp, Expansion};
    use crate::extensions::ShellExtensions;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Configurable test fixture for filter behavior testing.
    // Replaces multiple separate extension structs with a single builder-pattern design.

    /// Defines how the pre-filter should behave in tests.
    #[derive(Clone)]
    enum PreFilterBehavior {
        Passthrough,
        ModifyInput(String),
        ReturnSuccess(String),
        ReturnError(String),
    }

    /// Defines how the post-filter should behave in tests.
    #[derive(Clone)]
    enum PostFilterBehavior {
        Passthrough,
        ModifyOutput(String),
        ConvertSuccessToError(String),
        RecoverFromError(String),
    }

    /// Configurable test extension that can simulate various filter scenarios.
    /// The `post_called` flag allows tests to verify whether the post-filter was invoked.
    #[derive(Clone)]
    struct TestExtensions {
        pre_behavior: PreFilterBehavior,
        post_behavior: PostFilterBehavior,
        post_called: Arc<AtomicBool>,
    }

    impl TestExtensions {
        fn new() -> Self {
            Self {
                pre_behavior: PreFilterBehavior::Passthrough,
                post_behavior: PostFilterBehavior::Passthrough,
                post_called: Arc::new(AtomicBool::new(false)),
            }
        }

        fn with_pre_behavior(mut self, behavior: PreFilterBehavior) -> Self {
            self.pre_behavior = behavior;
            self
        }

        fn with_post_behavior(mut self, behavior: PostFilterBehavior) -> Self {
            self.post_behavior = behavior;
            self
        }

        fn post_was_called(&self) -> bool {
            self.post_called.load(Ordering::SeqCst)
        }

        /// Converts an owned string to a static lifetime reference.
        /// This is necessary because `pre_expand_word` returns `&'a str` where the lifetime
        /// is tied to the input parameter, but we need to return modified strings.
        /// The leaked memory is acceptable in tests.
        fn get_static_str(s: &str) -> &'static str {
            Box::leak(s.to_owned().into_boxed_str())
        }
    }

    impl ShellExtensions for TestExtensions {
        fn pre_expand_word<'a>(&self, input: &'a str) -> PreFilterResult<ExpandWordOp<'a>> {
            match &self.pre_behavior {
                PreFilterBehavior::Passthrough => PreFilterResult::Continue(input),
                PreFilterBehavior::ModifyInput(new_input) => {
                    PreFilterResult::Continue(Self::get_static_str(new_input))
                }
                PreFilterBehavior::ReturnSuccess(s) => {
                    PreFilterResult::Return(Ok(Expansion::from(s.clone())))
                }
                PreFilterBehavior::ReturnError(msg) => {
                    PreFilterResult::Return(Err(crate::error::Error::from(
                        crate::error::ErrorKind::CheckedExpansionError(msg.clone()),
                    )))
                }
            }
        }

        fn post_expand_word<'a>(
            &self,
            output: <ExpandWordOp<'a> as FilterableOp>::Output,
        ) -> PostFilterResult<ExpandWordOp<'a>> {
            // Track that post-filter was called, regardless of behavior.
            // This allows tests to verify filter bypass logic.
            self.post_called.store(true, Ordering::SeqCst);

            match &self.post_behavior {
                PostFilterBehavior::Passthrough => PostFilterResult::Return(output),
                PostFilterBehavior::ModifyOutput(s) => {
                    PostFilterResult::Return(Ok(Expansion::from(s.clone())))
                }
                PostFilterBehavior::ConvertSuccessToError(msg) => {
                    PostFilterResult::Return(Err(crate::error::Error::from(
                        crate::error::ErrorKind::CheckedExpansionError(msg.clone()),
                    )))
                }
                PostFilterBehavior::RecoverFromError(s) => {
                    PostFilterResult::Return(Ok(Expansion::from(s.clone())))
                }
            }
        }

        fn clone_for_subshell(&self) -> Box<dyn ShellExtensions> {
            Box::new(Self {
                pre_behavior: self.pre_behavior.clone(),
                post_behavior: self.post_behavior.clone(),
                // Share the same post_called flag so we can track calls across clones
                post_called: Arc::clone(&self.post_called),
            })
        }
    }

    struct MockShell<E: ShellExtensions> {
        ext: E,
    }

    impl<E: ShellExtensions> MockShell<E> {
        fn extensions(&self) -> &E {
            &self.ext
        }
    }

    #[test]
    fn test_with_filter_passthrough_preserves_input_and_output() {
        let ext = TestExtensions::new();
        let shell = MockShell { ext };

        let result =
            crate::with_filter!(&shell, pre_expand_word, post_expand_word, "test", |word| {
                Ok::<Expansion, crate::error::Error>(Expansion::from(format!("processed_{word}")))
            })
            .unwrap();

        assert_eq!(String::from(result), "processed_test");
    }

    #[test]
    fn test_with_filter_pre_modifies_input() {
        let ext = TestExtensions::new()
            .with_pre_behavior(PreFilterBehavior::ModifyInput("modified_input".to_string()));
        let shell = MockShell { ext };

        let result = crate::with_filter!(
            &shell,
            pre_expand_word,
            post_expand_word,
            "original",
            |word| {
                Ok::<Expansion, crate::error::Error>(Expansion::from(format!("processed_{word}")))
            }
        )
        .unwrap();

        assert_eq!(String::from(result), "processed_modified_input");
    }

    #[test]
    fn test_with_filter_pre_bypass_skips_body_and_post() {
        let ext = TestExtensions::new()
            .with_pre_behavior(PreFilterBehavior::ReturnSuccess("bypassed".to_string()));
        let shell = MockShell { ext: ext.clone() };

        #[allow(clippy::assertions_on_constants)]
        let result = crate::with_filter!(
            &shell,
            pre_expand_word,
            post_expand_word,
            "original",
            |_word| {
                assert!(false, "body should not be called when pre-filter bypasses");
                Ok::<Expansion, crate::error::Error>(Expansion::default())
            }
        )
        .unwrap();

        assert_eq!(String::from(result), "bypassed");
        assert!(
            !ext.post_was_called(),
            "post-filter should not be called when pre-filter bypasses"
        );
    }

    #[test]
    fn test_with_filter_post_passthrough_preserves_output() {
        let ext = TestExtensions::new();
        let shell = MockShell { ext };

        let result =
            crate::with_filter!(&shell, pre_expand_word, post_expand_word, "input", |word| {
                Ok::<Expansion, crate::error::Error>(Expansion::from(format!("result_{word}")))
            })
            .unwrap();

        assert_eq!(String::from(result), "result_input");
    }

    #[test]
    fn test_with_filter_post_modifies_output() {
        let ext = TestExtensions::new().with_post_behavior(PostFilterBehavior::ModifyOutput(
            "altered_output".to_string(),
        ));
        let shell = MockShell { ext };

        let result =
            crate::with_filter!(&shell, pre_expand_word, post_expand_word, "input", |word| {
                Ok::<Expansion, crate::error::Error>(Expansion::from(format!("result_{word}")))
            })
            .unwrap();

        assert_eq!(String::from(result), "altered_output");
    }

    #[test]
    fn test_with_filter_pre_bypass_with_error() {
        let ext = TestExtensions::new().with_pre_behavior(PreFilterBehavior::ReturnError(
            "pre-filter error".to_string(),
        ));
        let shell = MockShell { ext: ext.clone() };

        #[allow(clippy::assertions_on_constants)]
        let result: Result<Expansion, crate::error::Error> = crate::with_filter!(
            &shell,
            pre_expand_word,
            post_expand_word,
            "input",
            |_word| {
                assert!(
                    false,
                    "body should not be called when pre-filter returns error"
                );
                Ok::<Expansion, crate::error::Error>(Expansion::default())
            }
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.kind(),
            crate::error::ErrorKind::CheckedExpansionError(msg) if msg == "pre-filter error"
        ));
        assert!(
            !ext.post_was_called(),
            "post-filter should not be called when pre-filter returns error"
        );
    }

    #[test]
    fn test_with_filter_body_error_propagates() {
        let ext = TestExtensions::new();
        let shell = MockShell { ext: ext.clone() };

        let result: Result<Expansion, crate::error::Error> = crate::with_filter!(
            &shell,
            pre_expand_word,
            post_expand_word,
            "input",
            |_word| {
                Err::<Expansion, crate::error::Error>(crate::error::Error::from(
                    crate::error::ErrorKind::CheckedExpansionError("body error".to_string()),
                ))
            }
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.kind(),
            crate::error::ErrorKind::CheckedExpansionError(msg) if msg == "body error"
        ));
        assert!(
            ext.post_was_called(),
            "post-filter should be called even when body returns error"
        );
    }

    #[test]
    fn test_with_filter_post_preserves_error() {
        let ext = TestExtensions::new();
        let shell = MockShell { ext };

        let result: Result<Expansion, crate::error::Error> = crate::with_filter!(
            &shell,
            pre_expand_word,
            post_expand_word,
            "input",
            |_word| {
                Err::<Expansion, crate::error::Error>(crate::error::Error::from(
                    crate::error::ErrorKind::CheckedExpansionError("original error".to_string()),
                ))
            }
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.kind(),
            crate::error::ErrorKind::CheckedExpansionError(msg) if msg == "original error"
        ));
    }

    #[test]
    fn test_with_filter_post_recovers_from_error() {
        let ext = TestExtensions::new().with_post_behavior(PostFilterBehavior::RecoverFromError(
            "recovered".to_string(),
        ));
        let shell = MockShell { ext };

        let result: Result<Expansion, crate::error::Error> = crate::with_filter!(
            &shell,
            pre_expand_word,
            post_expand_word,
            "input",
            |_word| {
                Err::<Expansion, crate::error::Error>(crate::error::Error::from(
                    crate::error::ErrorKind::CheckedExpansionError("body error".to_string()),
                ))
            }
        );

        assert!(result.is_ok());
        let expansion = result.unwrap();
        assert_eq!(String::from(expansion), "recovered");
    }

    #[test]
    fn test_with_filter_post_converts_success_to_error() {
        let ext = TestExtensions::new().with_post_behavior(
            PostFilterBehavior::ConvertSuccessToError("validation failed".to_string()),
        );
        let shell = MockShell { ext };

        let result =
            crate::with_filter!(&shell, pre_expand_word, post_expand_word, "input", |word| {
                Ok::<Expansion, crate::error::Error>(Expansion::from(format!("result_{word}")))
            });

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.kind(),
            crate::error::ErrorKind::CheckedExpansionError(msg) if msg == "validation failed"
        ));
    }

    #[test]
    fn test_with_filter_combined_pre_and_post_modifications() {
        let ext = TestExtensions::new()
            .with_pre_behavior(PreFilterBehavior::ModifyInput("pre_modified".to_string()))
            .with_post_behavior(PostFilterBehavior::ModifyOutput("final_output".to_string()));
        let shell = MockShell { ext };

        let result =
            crate::with_filter!(&shell, pre_expand_word, post_expand_word, "input", |word| {
                // Verify pre-filter modified the input
                assert_eq!(word, "pre_modified");
                Ok::<Expansion, crate::error::Error>(Expansion::from(format!("processed_{word}")))
            })
            .unwrap();

        // Verify post-filter modified the output
        assert_eq!(String::from(result), "final_output");
    }

    #[test]
    fn test_with_filter_combined_with_error_in_middle() {
        let ext = TestExtensions::new()
            .with_pre_behavior(PreFilterBehavior::ModifyInput("pre_modified".to_string()));
        let shell = MockShell { ext: ext.clone() };

        let result: Result<Expansion, crate::error::Error> =
            crate::with_filter!(&shell, pre_expand_word, post_expand_word, "input", |word| {
                // Verify pre-filter modified the input before error
                assert_eq!(word, "pre_modified");
                Err::<Expansion, crate::error::Error>(crate::error::Error::from(
                    crate::error::ErrorKind::CheckedExpansionError(
                        "error after pre-filter".to_string(),
                    ),
                ))
            });

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err.kind(),
            crate::error::ErrorKind::CheckedExpansionError(msg) if msg == "error after pre-filter"
        ));
        assert!(
            ext.post_was_called(),
            "post-filter should receive error from body"
        );
    }
}
