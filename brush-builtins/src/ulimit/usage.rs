//! `ulimit` builtin: `ULimitCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use super::LimitValue;
use brush_core::args::{ArgsError, FromArgs};

/// Modify shell resource limits.
///
/// Provides control over the resources available to the shell and processes
/// it creates, on systems that allow such control.
#[derive(usage::Cli, Debug)]
#[usage(bin = "ulimit", unknown_flags = "error", args_override_self = false)]
pub(crate) struct ULimitCommand {
    /// use the `soft` resource limit
    #[usage(short = 'S')]
    pub(super) soft: bool,
    /// use the `hard` resource limit
    #[usage(short = 'H')]
    pub(super) hard: bool,
    /// all current limits are reported
    #[usage(short = 'a')]
    pub(super) all: bool,

    // TODO(usage-migration): clap's `num_args(0..=1)` has no direct equivalent; `default_missing`
    // makes each flag's value optional instead. The dynamic "(supported)"/"(unsupported)" suffix
    // that clap rendered via `IntoResettable<StyledStr>` cannot be expressed in a static help
    // string, so it has been dropped.
    /// the maximum socket buffer size
    #[usage(short = 'b', default_missing = "", help = "the socket buffer size")]
    pub(super) sbsize: Option<LimitValue>,
    /// the maximum size of core files created
    #[usage(
        short = 'c',
        default_missing = "",
        help = "the maximum size of core files created"
    )]
    pub(super) core: Option<LimitValue>,
    /// the maximum size of a process's data segment
    #[usage(
        short = 'd',
        default_missing = "",
        help = "the maximum size of a process's data segment"
    )]
    pub(super) data: Option<LimitValue>,
    /// the maximum scheduling priority (`nice`)
    #[usage(
        short = 'e',
        default_missing = "",
        help = "the maximum scheduling priority (`nice`)"
    )]
    pub(super) nice: Option<LimitValue>,
    /// the maximum size of files written by the shell and its children
    #[usage(
        short = 'f',
        default_missing = "",
        help = "the maximum size of files written by the shell and its children"
    )]
    pub(super) file_size: Option<LimitValue>,
    /// the maximum number of pending signals
    #[usage(
        short = 'i',
        default_missing = "",
        help = "the maximum number of pending signals"
    )]
    pub(super) sigpending: Option<LimitValue>,
    /// the maximum size a process may lock into memory
    #[usage(
        short = 'l',
        default_missing = "",
        help = "the maximum size a process may lock into memory"
    )]
    pub(super) memlock: Option<LimitValue>,
    /// the maximum number of kqueues allocated for this process
    #[usage(
        short = 'k',
        default_missing = "",
        help = "the maximum number of kqueues allocated for this process"
    )]
    pub(super) kqueues: Option<LimitValue>,
    /// the maximum resident set size
    #[usage(
        short = 'm',
        default_missing = "",
        help = "the maximum resident set size"
    )]
    pub(super) rss: Option<LimitValue>,
    /// the maximum number of open file descriptors
    #[usage(
        short = 'n',
        default_missing = "",
        help = "the maximum number of open file descriptors"
    )]
    pub(super) file_open: Option<LimitValue>,
    /// the pipe buffer size
    #[usage(short = 'p', default_missing = "", help = "the pipe buffer size")]
    pub(super) pipe: Option<LimitValue>,
    /// the maximum number of bytes in POSIX message queues
    #[usage(
        short = 'q',
        default_missing = "",
        help = "the maximum number of bytes in POSIX message queues"
    )]
    pub(super) msgqueue: Option<LimitValue>,
    /// the maximum real-time scheduling priority
    #[usage(
        short = 'r',
        default_missing = "",
        help = "the maximum real-time scheduling priority"
    )]
    pub(super) rtprio: Option<LimitValue>,
    /// the maximum stack size
    #[usage(short = 's', default_missing = "", help = "the maximum stack size")]
    pub(super) stack: Option<LimitValue>,
    /// the maximum amount of cpu time in seconds
    #[usage(
        short = 't',
        default_missing = "",
        help = "the maximum amount of cpu time in seconds"
    )]
    pub(super) cpu: Option<LimitValue>,
    /// the size of virtual memory
    #[usage(
        short = 'u',
        default_missing = "",
        help = "the maximum number of user processes"
    )]
    pub(super) nproc: Option<LimitValue>,
    /// the size of virtual memory
    #[usage(short = 'v', default_missing = "", help = "the size of virtual memory")]
    pub(super) vmem: Option<LimitValue>,
    /// the maximum number of file locks
    #[usage(
        short = 'x',
        default_missing = "",
        help = "the maximum number of file locks"
    )]
    pub(super) file_lock: Option<LimitValue>,
    /// the maximum number of pseudoterminals
    #[usage(
        short = 'P',
        default_missing = "",
        help = "the maximum number of pseudoterminals"
    )]
    pub(super) npts: Option<LimitValue>,
    /// real-time non-blocking time
    #[usage(
        short = 'R',
        default_missing = "",
        help = "the maximum real-time scheduling priority"
    )]
    pub(super) rttime: Option<LimitValue>,
    /// the maximum number of threads
    #[usage(
        short = 'T',
        default_missing = "",
        help = "the maximum number of threads"
    )]
    pub(super) threads: Option<LimitValue>,

    /// argument for the implicit limit (`-f`)
    pub(super) limit: Option<LimitValue>,
}

crate::impl_usage_parse!(ULimitCommand);

impl FromArgs for ULimitCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for ULimitCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::usage_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
