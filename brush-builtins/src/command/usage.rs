//! `command` builtin: `CommandCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::{fmt::Display, io::Write, path::Path};
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Directly invokes an external command, without going through typical search order.
#[derive(Default, usage::Cli)]
#[usage(bin = "command", unknown_flags = "value", args_override_self = false)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    #[usage(short = 'p')]
    pub(super) use_default_path: bool,

    /// Display a short description of the command.
    #[usage(short = 'v')]
    pub(super) print_description: bool,

    /// Display a more verbose description of the command.
    #[usage(short = 'V')]
    pub(super) print_verbose_description: bool,

    /// Command and arguments.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) command_and_args: Vec<String>,
}

crate::impl_usage_parse!(CommandCommand);

impl FromArgs for CommandCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl CommandCommand {
    /// Returns the command to look up, if any.
    pub(crate) fn command(&self) -> Option<&str> {
        self.command_and_args.first().map(|s| s.as_str())
    }
}


impl builtins::Command for CommandCommand {
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
