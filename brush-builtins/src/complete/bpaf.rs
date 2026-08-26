//! `complete` builtin: `CompleteCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]


use brush_core::completion::Spec;


use bpaf::Parser;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::ExecutionExitCode;
use brush_core::ExecutionResult;
use brush_core::builtins;

pub(crate) struct CommonCompleteCommandArgs {
    pub(crate) options: Vec<brush_core::completion::CompleteOption>,
    pub(crate) actions: Vec<brush_core::completion::CompleteAction>,
    pub(crate) glob_pattern: Option<String>,
    pub(crate) word_list: Option<String>,
    pub(crate) function_name: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) filter_pattern: Option<String>,
    pub(crate) prefix: Option<String>,
    pub(crate) suffix: Option<String>,
    pub(crate) action_alias: bool,
    pub(crate) action_builtin: bool,
    pub(crate) action_command: bool,
    pub(crate) action_directory: bool,
    pub(crate) action_exported: bool,
    pub(crate) action_file: bool,
    pub(crate) action_group: bool,
    pub(crate) action_job: bool,
    pub(crate) action_keyword: bool,
    pub(crate) action_service: bool,
    pub(crate) action_user: bool,
    pub(crate) action_variable: bool,
}

/// Returns whether the given argument is one of the value-taking short options
/// whose values are permitted to look like flags.
fn is_value_taking_option(arg: &str) -> bool {
    matches!(arg, "-G" | "-W" | "-F" | "-C" | "-X" | "-P" | "-S")
}

/// Joins flag-looking values onto the value-taking options that precede them
/// (e.g., `-W -foo` becomes `-W=-foo`) since bpaf otherwise rejects separate
/// values that start with `-`.
fn join_flag_looking_values(args: Vec<String>) -> Vec<String> {
    let mut joined = Vec::with_capacity(args.len());
    let mut iter = args.into_iter().peekable();

    while let Some(arg) = iter.next() {
        if is_value_taking_option(&arg) && iter.peek().is_some_and(|next| next.starts_with('-')) {
            if let Some(next) = iter.next() {
                joined.push(format!("{arg}={next}"));
                continue;
            }
        }

        joined.push(arg);
    }

    joined
}

fn run_parser<T: crate::args::bpaf_support::BpafArgs>(args: &[String]) -> Result<T, ArgsError> {
    crate::args::bpaf_support::run_parser::<T>(args)
}

fn render_parse_failure(failure: bpaf::ParseFailure) -> ArgsError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => ArgsError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => ArgsError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => ArgsError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

/// Generate command completions.
pub(crate) struct CompGenCommand {
    common_args: CommonCompleteCommandArgs,

    // N.B. The word can only start with a hyphen if it's after a --.
    word: Option<String>,
}

/// Set programmable command completion options.
pub(crate) struct CompOptCommand {
    update_default: bool,
    update_empty: bool,
    update_initial_word: bool,
    enabled_options: Vec<brush_core::completion::CompleteOption>,
    disabled_options: Vec<brush_core::completion::CompleteOption>,
    names: Vec<String>,
}

/// Configure programmable command completion.
pub(crate) struct CompleteCommand {
    pub(super) print: bool,
    pub(super) remove: bool,
    pub(super) use_as_default: bool,
    pub(super) use_for_empty_line: bool,
    pub(super) use_for_initial_word: bool,
    pub(super) common_args: CommonCompleteCommandArgs,
    pub(super) names: Vec<String>,
}

impl CommonCompleteCommandArgs {
    fn parser() -> impl bpaf::Parser<Self> + 'static {
        let options = bpaf::short('o')
            .help("Options governing the behavior of completions.")
            .argument::<brush_core::completion::CompleteOption>("OPT")
            .many();
        let actions = bpaf::short('A')
            .help("Actions to apply to generate completions.")
            .argument::<brush_core::completion::CompleteAction>("ACTION")
            .many();
        let glob_pattern = bpaf::short('G')
            .help("File glob pattern to be expanded to generate completions.")
            .argument::<String>("GLOB")
            .optional();
        let word_list = bpaf::short('W')
            .help("List of words that will be considered as completions.")
            .argument::<String>("WORD_LIST")
            .optional();
        let function_name = bpaf::short('F')
            .help("Name of a shell function to invoke to generate completions.")
            .argument::<String>("FUNC_NAME")
            .optional();
        let command = bpaf::short('C')
            .help("Command to execute to generate completions.")
            .argument::<String>("COMMAND")
            .optional();
        let filter_pattern = bpaf::short('X')
            .help("Pattern used as filter for completions.")
            .argument::<String>("PATTERN")
            .optional();
        let prefix = bpaf::short('P')
            .help("Prefix pattern used as filter for completions.")
            .argument::<String>("PREFIX")
            .optional();
        let suffix = bpaf::short('S')
            .help("Suffix pattern used as filter for completions.")
            .argument::<String>("SUFFIX")
            .optional();

        let action_alias = bpaf::short('a')
            .help("Complete with valid aliases.")
            .switch();
        let action_builtin = bpaf::short('b')
            .help("Complete with names of shell builtins.")
            .switch();
        let action_command = bpaf::short('c')
            .help("Complete with names of executable commands.")
            .switch();
        let action_directory = bpaf::short('d')
            .help("Complete with directory names.")
            .switch();
        let action_exported = bpaf::short('e')
            .help("Complete with names of exported shell variables.")
            .switch();
        let action_file = bpaf::short('f').help("Complete with filenames.").switch();
        let action_group = bpaf::short('g')
            .help("Complete with valid user groups.")
            .switch();
        let action_job = bpaf::short('j').help("Complete with job specs.").switch();
        let action_keyword = bpaf::short('k').help("Complete with keywords.").switch();
        let action_service = bpaf::short('s')
            .help("Complete with names of system services.")
            .switch();
        let action_user = bpaf::short('u')
            .help("Complete with valid usernames.")
            .switch();
        let action_variable = bpaf::short('v')
            .help("Complete with names of shell variables.")
            .switch();

        bpaf::construct!(Self {
            options,
            actions,
            glob_pattern,
            word_list,
            function_name,
            command,
            filter_pattern,
            prefix,
            suffix,
            action_alias,
            action_builtin,
            action_command,
            action_directory,
            action_exported,
            action_file,
            action_group,
            action_job,
            action_keyword,
            action_service,
            action_user,
            action_variable,
        })
    }




}

impl crate::args::bpaf_support::BpafArgs for CompleteCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let print = bpaf::short('p')
            .help("Display registered completion settings.")
            .switch();
        let remove = bpaf::short('r')
            .help("Remove the completion settings associated with the given command.")
            .switch();
        let use_as_default = bpaf::short('D')
            .help("Apply these settings to the default completion scenario.")
            .switch();
        let use_for_empty_line = bpaf::short('E')
            .help("Apply these settings to completion of empty lines.")
            .switch();
        let use_for_initial_word = bpaf::short('I')
            .help("Apply these settings to completion of the initial word of the input line.")
            .switch();
        let common_args = CommonCompleteCommandArgs::parser();
        let names = bpaf::positional::<String>("NAMES")
            .help("Names of commands to configure completions for.")
            .many();

        bpaf::construct!(CompleteCommand {
            print,
            remove,
            use_as_default,
            use_for_empty_line,
            use_for_initial_word,
            common_args,
            names,
        })
    }
fn about() -> &'static str {
        "Configure programmable command completion."
    }
fn synopsis() -> &'static str {
        "[-prDEI] [-o OPT]... [-A ACTION]... [NAME]..."
    }
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();

        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        crate::args::bpaf_support::run_parser::<Self>(&join_flag_looking_values(args))
    
    }
}

impl FromArgs for CompleteCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}


impl crate::args::bpaf_support::BpafArgs for CompGenCommand {
    fn parser() -> impl bpaf::Parser<Self> {
        let common_args = CommonCompleteCommandArgs::parser();
        let word = bpaf::positional::<String>("WORD").optional();

        bpaf::construct!(CompGenCommand { common_args, word })
    }

    fn about() -> &'static str {
        "Configure programmable command completion."
    }

    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();
        let args: Vec<String> = args.into_iter().skip(1).collect();
        run_parser(&join_flag_looking_values(args))
    }
}

impl FromArgs for CompGenCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl crate::args::bpaf_support::BpafArgs for CompOptCommand {
    fn parser() -> impl bpaf::Parser<Self> {
        let update_default = bpaf::short('D')
            .help("Update the default completion settings.")
            .switch();
        let update_empty = bpaf::short('E')
            .help("Update the completion settings for empty lines.")
            .switch();
        let update_initial_word = bpaf::short('I')
            .help("Update the completion settings for the initial word of the input line.")
            .switch();

        let enabled_options = bpaf::short('o')
            .help("Enable the specified option for selected completion scenarios.")
            .argument::<brush_core::completion::CompleteOption>("OPT")
            .many();

        // N.B. The value may be adjacent to the tag (`+o OPT`); it cannot be
        // expressed as a simple argument parser because of the '+' spelling.
        let disabled_options = {
            let tag = bpaf::literal("+o");
            let val = bpaf::any("OPT", |opt: brush_core::completion::CompleteOption| Some(opt)).optional();
            bpaf::construct!(tag, val)
                .adjacent()
                .many()
                .map(|groups| groups.into_iter().filter_map(|((), opt)| opt).collect())
        };

        let names = bpaf::positional::<String>("NAMES")
            .help("If specified, scopes updates to completions of the named commands.")
            .many();

        bpaf::construct!(CompOptCommand {
            update_default,
            update_empty,
            update_initial_word,
            enabled_options,
            disabled_options,
            names,
        })
    }

    fn about() -> &'static str {
        "Configure programmable command completion."
    }

    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();
        let args: Vec<String> = args.into_iter().skip(1).collect();
        run_parser(&args)
    }
}

impl FromArgs for CompOptCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for CompleteCommand {
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
                return brush_core::error::unimp("restart completion");
            }
        }

        Ok(ExecutionResult::success())
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
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
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }
}
