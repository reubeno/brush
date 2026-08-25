//! `wait` builtin: `WaitCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Bpaf;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Wait for jobs to terminate.
#[derive(Bpaf)]
pub(crate) struct WaitCommand {
    /// Wait for specified job to terminate (instead of change status).
    #[bpaf(short('f'))]
    pub(super) wait_for_terminate: bool,

    /// Wait for a single job to change status; if jobs are specified, waits for
    /// the first to change status, and otherwise waits for the next change.
    #[bpaf(short('n'))]
    pub(super) wait_for_first_or_next: bool,

    /// Name of variable to receive the job ID of the job whose status is indicated.
    #[bpaf(short('p'), argument("VAR_NAME"))]
    pub(super) variable_to_receive_id: Option<String>,

    /// Process IDs or job specs to wait for.
    #[bpaf(positional("IDS"))]
    pub(super) ids: Vec<String>,
}

impl crate::args::BpafArgs for WaitCommand {
fn parser() -> impl bpaf::Parser<Self> {
        wait_command()
    }
fn about() -> &'static str {
        "Wait for jobs to terminate."
    }
fn synopsis() -> &'static str {
        "[-fn] [-p VAR_NAME] [IDS]..."
    }
}

impl FromArgs for WaitCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for WaitCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
