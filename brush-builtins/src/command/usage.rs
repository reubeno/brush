//! `command` builtin: `CommandCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::{fmt::Display, io::Write};
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Directly invokes an external command, without going through typical search order.
#[derive(Default, usage::Cli)]
#[usage(bin = "command", unknown_flags = "value", args_override_self = false)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    #[usage(short = 'p')]
    pub(crate) use_default_path: bool,

    /// Display a short description of the command.
    #[usage(short = 'v')]
    pub(crate) print_description: bool,

    /// Display a more verbose description of the command.
    #[usage(short = 'V')]
    pub(crate) print_verbose_description: bool,

    /// Command and arguments.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    pub(crate) command_and_args: Vec<String>,
}

crate::impl_usage_parse!(CommandCommand);

impl FromArgs for CommandCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        let mut command: Self = crate::args::UsageArgs::from_words(words)?;

        // N.B. bash gives `-v`/`-V` last-wins semantics, but the switches
        // above only record presence. When both fired, resolve the tie from
        // the raw option words in order; clustered spellings (e.g. `-vV`)
        // count left-to-right, matching the parser's option-zone boundary.
        if command.print_description && command.print_verbose_description {
            // N.B. words[0] is the command name itself.
            let args: Vec<String> = words.iter().skip(1).cloned().collect();
            let (options, _) = crate::args::usage_support::split_option_section(&args, "", &[]);

            let mut last_is_verbose = false;
            for option in &options {
                if let Some(group) = option.strip_prefix('-') {
                    for c in group.chars() {
                        if c == 'v' {
                            last_is_verbose = false;
                        } else if c == 'V' {
                            last_is_verbose = true;
                        }
                    }
                }
            }

            command.print_description = !last_is_verbose;
            command.print_verbose_description = last_is_verbose;
        }

        Ok(command)
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
