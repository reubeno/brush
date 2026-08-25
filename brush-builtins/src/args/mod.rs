//! Per-engine runtime support for builtin argument modules.

#[cfg(feature = "parser-bpaf")]
pub(crate) mod bpaf_support;
#[cfg(feature = "parser-usage")]
pub(crate) mod usage_support;

#[cfg(feature = "parser-bpaf")]
pub(crate) use bpaf_support::{BpafArgs, get_content as get_content_bpaf};
#[cfg(feature = "parser-usage")]
pub(crate) use usage_support::UsageArgs;
