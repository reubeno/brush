//! `complete` builtin: `CompleteCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use std::io::Write;
use std::collections::HashMap;
use brush_core::{ExecutionExitCode, ExecutionResult, error};
use brush_core::completion::{CompleteAction, CompleteOption, Spec};
use brush_core::builtins;

/// Configure programmable command completion.
#[derive(Parser)]
pub(crate) struct CompleteCommand {
    /// Display registered completion settings.
    #[arg(short = 'p')]
    pub(super) print: bool,

    /// Remove the completion settings associated with the given command.
    #[arg(short = 'r')]
    pub(super) remove: bool,

    /// Apply these settings to the default completion scenario.
    #[arg(short = 'D')]
    pub(super) use_as_default: bool,

    /// Apply these settings to completion of empty lines.
    #[arg(short = 'E')]
    pub(super) use_for_empty_line: bool,

    /// Apply these settings to completion of the initial word of the input line.
    #[arg(short = 'I')]
    pub(super) use_for_initial_word: bool,

    #[clap(flatten)]
    pub(super) common_args: CommonCompleteCommandArgs,

    pub(super) names: Vec<String>,
}

#[derive(Parser)]

pub(super) struct CommonCompleteCommandArgs {
    /// Options governing the behavior of completions.
    #[arg(short = 'o')]
    pub(super) options: Vec<CompleteOption>,

    /// Actions to apply to generate completions.
    #[arg(short = 'A')]
    pub(super) actions: Vec<CompleteAction>,

    /// File glob pattern to be expanded to generate completions.
    #[arg(short = 'G', allow_hyphen_values = true, value_name = "GLOB")]
    pub(super) glob_pattern: Option<String>,

    /// List of words that will be considered as completions.
    #[arg(short = 'W', allow_hyphen_values = true)]
    pub(super) word_list: Option<String>,

    /// Name of a shell function to invoke to generate completions.
    #[arg(short = 'F', allow_hyphen_values = true, value_name = "FUNC_NAME")]
    pub(super) function_name: Option<String>,

    /// Command to execute to generate completions.
    #[arg(short = 'C', allow_hyphen_values = true)]
    pub(super) command: Option<String>,

    /// Pattern used as filter for completions.
    #[arg(short = 'X', allow_hyphen_values = true, value_name = "PATTERN")]
    pub(super) filter_pattern: Option<String>,

    /// Prefix pattern used as filter for completions.
    #[arg(short = 'P', allow_hyphen_values = true)]
    pub(super) prefix: Option<String>,

    /// Suffix pattern used as filter for completions.
    #[arg(short = 'S', allow_hyphen_values = true)]
    pub(super) suffix: Option<String>,

    /// Complete with valid aliases.
    #[arg(short = 'a')]
    pub(super) action_alias: bool,

    /// Complete with names of shell builtins.
    #[arg(short = 'b')]
    pub(super) action_builtin: bool,

    /// Complete with names of executable commands.
    #[arg(short = 'c')]
    pub(super) action_command: bool,

    /// Complete with directory names.
    #[arg(short = 'd')]
    pub(super) action_directory: bool,

    /// Complete with names of exported shell variables.
    #[arg(short = 'e')]
    pub(super) action_exported: bool,

    /// Complete with filenames.
    #[arg(short = 'f')]
    pub(super) action_file: bool,

    /// Complete with valid user groups.
    #[arg(short = 'g')]
    pub(super) action_group: bool,

    /// Complete with job specs.
    #[arg(short = 'j')]
    pub(super) action_job: bool,

    /// Complete with keywords.
    #[arg(short = 'k')]
    pub(super) action_keyword: bool,

    /// Complete with names of system services.
    #[arg(short = 's')]
    pub(super) action_service: bool,

    /// Complete with valid usernames.
    #[arg(short = 'u')]
    pub(super) action_user: bool,

    /// Complete with names of shell variables.
    #[arg(short = 'v')]
    pub(super) action_variable: bool,
}

/// Generate command completions.
#[derive(Parser)]

pub(crate) struct CompGenCommand {
    #[clap(flatten)]
    pub(super) common_args: CommonCompleteCommandArgs,

    // N.B. The word can only start with a hyphen if it's after a --.
    pub(super) word: Option<String>,
}

/// Set programmable command completion options.
#[derive(Parser)]

pub(crate) struct CompOptCommand {
    /// Update the default completion settings.
    #[arg(short = 'D')]
    pub(super) update_default: bool,

    /// Update the completion settings for empty lines.
    #[arg(short = 'E')]
    pub(super) update_empty: bool,

    /// Update the completion settings for the initial word of the input line.
    #[arg(short = 'I')]
    pub(super) update_initial_word: bool,

    /// Enable the specified option for selected completion scenarios.
    #[arg(short = 'o', value_name = "OPT")]
    pub(super) enabled_options: Vec<CompleteOption>,
    #[arg(long = concat!("+o"), hide = true)]
    pub(super) disabled_options: Vec<CompleteOption>,

    /// If specified, scopes updates to completions of the named commands.
    pub(super) names: Vec<String>,
}

impl builtins::Command for CompleteCommand {
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

impl builtins::Command for CompGenCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut spec = self
            .common_args
            .create_spec(context.shell.options().extended_globbing);
        spec.options.no_sort = true;

        let token_to_complete = self.word.as_deref().unwrap_or_default();

        // We unquote the token-to-be-completed before passing it to the completion system.
        let unquoted_token = brush_parser::unquote_str(token_to_complete);

        let completion_context = brush_core::completion::Context {
            token_to_complete: unquoted_token.as_str(),
            preceding_token: None,
            command_name: None,
            token_index: 0,
            tokens: &[&brush_core::completion::CompletionToken {
                text: token_to_complete,
                start: 0,
            }],
            input_line: token_to_complete,
            cursor_index: token_to_complete.len(),
            trigger: brush_core::completion::CompletionTrigger::Programmatic,
        };

        let result = spec
            .get_completions(context.shell, &completion_context)
            .await?;

        match result {
            brush_core::completion::Answer::Candidates(candidates, _options) => {
                // We are expected to return 1 if there are no candidates, even if no errors
                // occurred along the way.
                if candidates.is_empty() {
                    return Ok(ExecutionResult::general_error());
                }

                for candidate in candidates {
                    writeln!(context.stdout(), "{candidate}")?;
                }
            }
            brush_core::completion::Answer::RestartCompletionProcess => {
                return error::unimp("restart completion");
            }
        }

        Ok(ExecutionResult::success())
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

impl builtins::Command for CompOptCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut options =
            HashMap::with_capacity(self.disabled_options.len() + self.enabled_options.len());
        for option in &self.disabled_options {
            options.insert(option.clone(), false);
        }
        for option in &self.enabled_options {
            options.insert(option.clone(), true);
        }

        if !self.names.is_empty() {
            if self.update_default || self.update_empty || self.update_initial_word {
                writeln!(
                    context.stderr(),
                    "compopt: cannot specify names with -D, -E, or -I"
                )?;
                return Ok(ExecutionExitCode::InvalidUsage.into());
            }

            for name in &self.names {
                let spec = context.shell.completion_config_mut().get_or_add_mut(name);
                Self::set_options_for_spec(spec, &options);
            }
        } else if self.update_default {
            if let Some(spec) = &mut context.shell.completion_config_mut().default {
                Self::set_options_for_spec(spec, &options);
            } else {
                let mut spec = Spec::default();
                Self::set_options_for_spec(&mut spec, &options);
                context.shell.completion_config_mut().default = Some(spec);
            }
        } else if self.update_empty {
            if let Some(spec) = &mut context.shell.completion_config_mut().empty_line {
                Self::set_options_for_spec(spec, &options);
            } else {
                let mut spec = Spec::default();
                Self::set_options_for_spec(&mut spec, &options);
                context.shell.completion_config_mut().empty_line = Some(spec);
            }
        } else if self.update_initial_word {
            if let Some(spec) = &mut context.shell.completion_config_mut().initial_word {
                Self::set_options_for_spec(spec, &options);
            } else {
                let mut spec = Spec::default();
                Self::set_options_for_spec(&mut spec, &options);
                context.shell.completion_config_mut().initial_word = Some(spec);
            }
        } else {
            // If we got here, then we need to apply to any completion actively in-flight.
            if let Some(in_flight_options) = context
                .shell
                .completion_config_mut()
                .current_completion_options
                .as_mut()
            {
                Self::set_options(in_flight_options, &options);
            }
        }

        Ok(ExecutionResult::success())
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
