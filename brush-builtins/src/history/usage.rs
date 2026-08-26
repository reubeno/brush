//! `history` builtin: `HistoryCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

#[derive(usage::Cli)]
#[usage(bin = "history", unknown_flags = "error", args_override_self = false)]
pub(crate) struct HistoryCommand {
    /// Clears all history.
    #[usage(short = 'c')]
    pub(super) clear_history: bool,

    /// Deletes the history entry at the given offset. Positive offsets are relative to the
    /// beginning of the history, while negative offsets are relative to the end of the history.
    #[usage(short = 'd', value_name = "OFFSET", allow_negative_numbers)]
    pub(super) delete_offset: Option<i64>,

    /// Appends the history from the current session to the history file.
    #[usage(short = 'a', value_name = "HIST_FILE")]
    pub(super) append_session_to_file: Option<Option<String>>,

    /// Appends any remaining history from the history file to the current session.
    #[usage(short = 'n', value_name = "HIST_FILE")]
    pub(super) append_rest_of_file_to_session: Option<Option<String>>,

    /// Appends the history from the history file to the current session.
    #[usage(short = 'r', value_name = "HIST_FILE")]
    pub(super) append_file_to_session: Option<Option<String>>,

    /// Replaces the history file with the current session history.
    #[usage(short = 'w', value_name = "HIST_FILE")]
    pub(super) write_session_to_file: Option<Option<String>>,

    /// History-expands positional arguments and displays them.
    #[usage(short = 'p', variadic, value_name = "ARG")]
    pub(super) expand_args: Option<Vec<String>>,

    /// Appends positional arguments as an entry in the current session.
    #[usage(short = 's', variadic, value_name = "ARG")]
    pub(super) append_args_to_session: Option<Vec<String>>,

    /// Arguments.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) args: Vec<String>,
}

crate::impl_usage_parse!(HistoryCommand);

impl FromArgs for HistoryCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for HistoryCommand {
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
