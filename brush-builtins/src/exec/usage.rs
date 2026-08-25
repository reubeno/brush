//! `exec` builtin: `ExecCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::{borrow::Cow, os::unix::process::CommandExt};
use brush_core::{ErrorKind, ExecutionExitCode, ExecutionResult, builtins, commands};
use brush_core::args::{ArgsError, FromArgs};

/// Exec the provided command.
#[derive(usage::Cli)]
#[usage(bin = "exec", unknown_flags = "value", args_override_self = false)]
pub(crate) struct ExecCommand {
    /// Pass given name as zeroth argument to command.
    #[usage(short = 'a', value_name = "NAME")]
    pub(super) name_for_argv0: Option<String>,

    /// Exec command with an empty environment.
    #[usage(short = 'c')]
    pub(super) empty_environment: bool,

    /// Exec command as a login shell.
    #[usage(short = 'l')]
    pub(super) exec_as_login: bool,

    /// Command and args.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(super) args: Vec<String>,
}

crate::impl_usage_parse!(ExecCommand);

impl FromArgs for ExecCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for ExecCommand {
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
