//! `ulimit` builtin: `ULimitCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]
use super::{LimitValue, ResourceDescription};

use clap::Parser;
use brush_core::builtins;

/// Modify shell resource limits.
///
/// Provides control over the resources available to the shell and processes
/// it creates, on systems that allow such control.
#[derive(Parser, Debug)]
pub(crate) struct ULimitCommand {
    /// use the `soft` resource limit
    #[arg(short = 'S')]
    pub(super) soft: bool,
    /// use the `hard` resource limit
    #[arg(short = 'H')]
    pub(super) hard: bool,
    /// all current limits are reported
    #[arg(short = 'a')]
    pub(super) all: bool,
    /// the maximum socket buffer size
    #[arg(short = 'b', default_missing_value = "", num_args(0..=1), help = ResourceDescription::SBSIZE)]
    pub(super) sbsize: Option<LimitValue>,
    /// the maximum size of core files created
    #[arg(short = 'c', default_missing_value = "", num_args(0..=1), help = ResourceDescription::CORE)]
    pub(super) core: Option<LimitValue>,
    /// the maximum size of a process's data segment
    #[arg(short = 'd', default_missing_value = "", num_args(0..=1), help = ResourceDescription::DATA)]
    pub(super) data: Option<LimitValue>,
    /// the maximum scheduling priority (`nice`)
    #[arg(short = 'e', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NICE)]
    pub(super) nice: Option<LimitValue>,
    /// the maximum size of files written by the shell and its children
    #[arg(short = 'f', default_missing_value = "", num_args(0..=1), help = ResourceDescription::FSIZE)]
    pub(super) file_size: Option<LimitValue>,
    /// the maximum number of pending signals
    #[arg(short = 'i', default_missing_value = "", num_args(0..=1), help = ResourceDescription::SIGPENDING)]
    pub(super) sigpending: Option<LimitValue>,
    /// the maximum size a process may lock into memory
    #[arg(short = 'l', default_missing_value = "", num_args(0..=1), help = ResourceDescription::MEMLOCK)]
    pub(super) memlock: Option<LimitValue>,
    /// the maximum number of kqueues allocated for this process
    #[arg(short = 'k', default_missing_value = "", num_args(0..=1), help = ResourceDescription::KQUEUES)]
    pub(super) kqueues: Option<LimitValue>,
    /// the maximum resident set size
    #[arg(short = 'm', default_missing_value = "", num_args(0..=1), help = ResourceDescription::RSS)]
    pub(super) rss: Option<LimitValue>,
    /// the maximum number of open file descriptors
    #[arg(short = 'n', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NOFILE)]
    pub(super) file_open: Option<LimitValue>,
    /// the pipe buffer size
    #[arg(short = 'p', default_missing_value = "", num_args(0..=1), help = ResourceDescription::PIPE)]
    pub(super) pipe: Option<LimitValue>,
    /// the maximum number of bytes in POSIX message queues
    #[arg(short = 'q', default_missing_value = "", num_args(0..=1), help = ResourceDescription::MSGQUEUE)]
    pub(super) msgqueue: Option<LimitValue>,
    /// the maximum real-time scheduling priority
    #[arg(short = 'r', default_missing_value = "", num_args(0..=1), help = ResourceDescription::RTPRIO)]
    pub(super) rtprio: Option<LimitValue>,
    /// the maximum stack size
    #[arg(short = 's', default_missing_value = "", num_args(0..=1), help = ResourceDescription::STACK)]
    pub(super) stack: Option<LimitValue>,
    /// the maximum amount of cpu time in seconds
    #[arg(short = 't', default_missing_value = "", num_args(0..=1), help = ResourceDescription::CPU)]
    pub(super) cpu: Option<LimitValue>,
    /// the size of virtual memory
    #[arg(short = 'u', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NPROC)]
    pub(super) nproc: Option<LimitValue>,
    /// the size of virtual memory
    #[arg(short = 'v', default_missing_value = "", num_args(0..=1), help = ResourceDescription::VMEM)]
    pub(super) vmem: Option<LimitValue>,
    /// the maximum number of file locks
    #[arg(short = 'x', default_missing_value = "", num_args(0..=1), help = ResourceDescription::LOCKS)]
    pub(super) file_lock: Option<LimitValue>,
    /// the maximum number of pseudoterminals
    #[arg(short = 'P', default_missing_value = "", num_args(0..=1), help = ResourceDescription::NPTS)]
    pub(super) npts: Option<LimitValue>,
    /// real-time non-blocking time
    #[arg(short = 'R', default_missing_value = "", num_args(0..=1), help = ResourceDescription::RTTIME)]
    pub(super) rttime: Option<LimitValue>,
    /// the maximum number of threads
    #[arg(short = 'T', default_missing_value = "", num_args(0..=1), help = ResourceDescription::THREADS)]
    pub(super) threads: Option<LimitValue>,

    /// argument for the implicit limit (`-f`)
    pub(super) limit: Option<LimitValue>,
}

impl builtins::Command for ULimitCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: help still rendered from clap-derived metadata.
        builtins::clap_content::<Self>(name, &content_type, options)
    }
}
