//! Per-engine runtime support for builtin argument modules.

// N.B. bpaf support is only needed (and compilable) when the bpaf engine is
// selected; see `arg_impl!` for engine selection priority.
#[cfg(all(feature = "parser-bpaf", not(feature = "parser-usage")))]
pub(crate) mod bpaf_support;
#[cfg(feature = "parser-usage")]
pub(crate) mod usage_support;

#[cfg(feature = "parser-usage")]
pub(crate) use usage_support::UsageArgs;
