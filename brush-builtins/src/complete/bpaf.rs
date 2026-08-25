//! `complete` builtin: `CompleteCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

use bpaf::Parser;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::Write;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

pub(crate) struct CommonCompleteCommandArgs {
    options: Vec<brush_core::completion::CompleteOption>,
    actions: Vec<brush_core::completion::CompleteAction>,
    glob_pattern: Option<String>,
    word_list: Option<String>,
    function_name: Option<String>,
    command: Option<String>,
    filter_pattern: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    action_alias: bool,
    action_builtin: bool,
    action_command: bool,
    action_directory: bool,
    action_exported: bool,
    action_file: bool,
    action_group: bool,
    action_job: bool,
    action_keyword: bool,
    action_service: bool,
    action_user: bool,
    action_variable: bool,
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

fn run_parser<T: builtins::Command>(args: &[String]) -> Result<T, ArgsError> {
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    T::parser()
        .to_options()
        .run_inner(os_args.as_slice())
        .map_err(render_parse_failure)
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

    fn create_spec(&self, extglob_enabled: bool) -> completion::Spec {
        let filter_pattern_excludes;
        let filter_pattern = if let Some(filter_pattern) = self.filter_pattern.as_ref() {
            // If the pattern starts with a '!' that's not the start of an extglob pattern,
            // then we invert.
            if let Some(remaining_pattern) = filter_pattern.strip_prefix('!') {
                if !extglob_enabled || !remaining_pattern.starts_with('(') {
                    filter_pattern_excludes = false;
                    Some(remaining_pattern.to_owned())
                } else {
                    filter_pattern_excludes = true;
                    Some(filter_pattern.to_owned())
                }
            } else {
                filter_pattern_excludes = true;
                Some(filter_pattern.clone())
            }
        } else {
            filter_pattern_excludes = false;
            None
        };

        let mut spec = completion::Spec {
            options: completion::GenerationOptions::default(),
            actions: self.resolve_actions(),
            glob_pattern: self.glob_pattern.clone(),
            word_list: self.word_list.clone(),
            function_name: self.function_name.clone(),
            command: self.command.clone(),
            filter_pattern,
            filter_pattern_excludes,
            prefix: self.prefix.clone(),
            suffix: self.suffix.clone(),
        };

        for option in &self.options {
            match option {
                brush_core::completion::CompleteOption::BashDefault => spec.options.bash_default = true,
                brush_core::completion::CompleteOption::Default => spec.options.default = true,
                brush_core::completion::CompleteOption::DirNames => spec.options.dir_names = true,
                brush_core::completion::CompleteOption::FileNames => spec.options.file_names = true,
                brush_core::completion::CompleteOption::NoQuote => spec.options.no_quote = true,
                brush_core::completion::CompleteOption::NoSort => spec.options.no_sort = true,
                brush_core::completion::CompleteOption::NoSpace => spec.options.no_space = true,
                brush_core::completion::CompleteOption::PlusDirs => spec.options.plus_dirs = true,
            }
        }

        spec
    }

    fn resolve_actions(&self) -> Vec<brush_core::completion::CompleteAction> {
        let mut actions = self.actions.clone();

        actions.extend(
            [
                (self.action_alias, brush_core::completion::CompleteAction::Alias),
                (self.action_builtin, brush_core::completion::CompleteAction::Builtin),
                (self.action_command, brush_core::completion::CompleteAction::Command),
                (self.action_directory, brush_core::completion::CompleteAction::Directory),
                (self.action_exported, brush_core::completion::CompleteAction::Export),
                (self.action_file, brush_core::completion::CompleteAction::File),
                (self.action_group, brush_core::completion::CompleteAction::Group),
                (self.action_job, brush_core::completion::CompleteAction::Job),
                (self.action_keyword, brush_core::completion::CompleteAction::Keyword),
                (self.action_service, brush_core::completion::CompleteAction::Service),
                (self.action_user, brush_core::completion::CompleteAction::User),
                (self.action_variable, brush_core::completion::CompleteAction::Variable),
            ]
            .into_iter()
            .filter_map(|(enabled, action)| enabled.then_some(action)),
        );

        actions
    }
}

impl crate::args::BpafArgs for CompleteCommand {
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
        crate::args::run_parser::<Self>(&join_flag_looking_values(args))
    
    }
}

impl FromArgs for CompleteCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for CompleteCommand {
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
