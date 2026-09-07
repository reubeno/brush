//! `exec` builtin: `ExecCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Exec the provided command.
pub(crate) struct ExecCommand {
    /// Pass given name as zeroth argument to command.
    pub(super) name_for_argv0: Option<String>,

    /// Exec command with an empty environment.
    pub(super) empty_environment: bool,

    /// Exec command as a login shell.
    pub(super) exec_as_login: bool,

    /// Command and args.
    pub(super) args: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for ExecCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let name_for_argv0 = bpaf::short('a')
            .help("Pass given name as zeroth argument to command.")
            .argument::<String>("NAME")
            .optional();
        let empty_environment = bpaf::short('c')
            .help("Exec command with an empty environment.")
            .switch();
        let exec_as_login = bpaf::short('l')
            .help("Exec command as a login shell.")
            .switch();
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(ExecCommand {
            name_for_argv0,
            empty_environment,
            exec_as_login,
            args,
        })
    }
fn about() -> &'static str {
        "Exec the provided command."
    }
fn synopsis() -> &'static str {
        "[-acl] [COMMAND [ARG]...]"
    }
fn takes_trailing_args() -> bool {
        true
    }
fn value_taking_short_options() -> &'static str {
        "a"
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        self.args = args;
    }
}

impl FromArgs for ExecCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for ExecCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
