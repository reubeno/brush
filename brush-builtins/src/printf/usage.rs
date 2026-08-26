//! `printf` builtin: `PrintfCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

#![allow(unused_imports, reason = "transitional engine scaffolding")]
#![allow(dead_code, reason = "transitional engine scaffolding")]

use std::io::Write;
use brush_core::builtins;
use brush_core::args::{ArgsError, FromArgs};

/// Format a string.
#[derive(usage::Cli)]
#[usage(
    bin = "printf",
    unknown_flags = "error",
    args_override_self = false,
    disable_help_flag,
    disable_version_flag
)]
pub(crate) struct PrintfCommand {
    /// If specified, the output of the command is assigned to this variable.
    #[usage(short = 'v')]
    pub(super) output_variable: Option<String>,

    /// Format string + arguments to the format string.
    ///
    /// N.B. We intentionally do *not* enable `allow_hyphen_values` here. Doing so would
    /// cause an attached short-option value such as `-va` (i.e. `-v a`) to be misparsed as
    /// a positional argument. With it disabled, a format string that genuinely needs to
    /// start with a hyphen must be preceded by `--`, matching other shells' behavior.
    #[usage(trailing_var_arg, required)]
    pub(super) format_and_args: Vec<String>,
}

crate::impl_usage_parse!(PrintfCommand);

impl FromArgs for PrintfCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        // Bash's `printf` accepts only a leading `-v` option zone and drops at
        // most one leading `--`; every later token (including further `--`s) is
        // verbatim data. The engine's generic argv parser strips all of them,
        // so scan the option zone manually and keep the remainder intact.
        let mut output_variable: Option<String> = None;
        let mut idx = 1; // N.B. words[0] is the command name.

        if words.get(idx).map(String::as_str) == Some("--") {
            idx += 1;
        } else {
            while let Some(word) = words.get(idx) {
                let word = word.as_str();
                if word == "-v" {
                    idx += 1;
                    match words.get(idx) {
                        Some(value) => {
                            output_variable = Some(value.clone());
                            idx += 1;
                        }
                        None => {
                            return Err(ArgsError {
                                message: "printf: -v: option requires an argument\n"
                                    .to_string(),
                                help_request: false,
                            });
                        }
                    }
                } else if let Some(value) = word.strip_prefix("-v") {
                    if !value.is_empty() && !value.starts_with('-') {
                        output_variable = Some(value.to_string());
                        idx += 1;
                    } else {
                        break;
                    }
                } else if word.starts_with('-') && word != "-" && word != "--" {
                    // Bash rejects unknown leading options outright.
                    let ch = word.chars().nth(1).unwrap_or('-');
                    return Err(ArgsError {
                        message: format!("printf: invalid option -- '{ch}'\n"),
                        help_request: false,
                    });
                } else {
                    break;
                }
            }
        }

        let format_and_args = words[idx..].to_vec();
        if format_and_args.is_empty() {
            return Err(ArgsError {
                message: "printf: usage: printf [-v var] format [arguments]\n".to_string(),
                help_request: false,
            });
        }

        Ok(Self {
            output_variable,
            format_and_args,
        })
    }
}

impl builtins::Command for PrintfCommand {
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
