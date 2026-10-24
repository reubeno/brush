use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use strum::IntoEnumIterator;

use brush_core::completion::{self, CompleteAction, CompleteOption, Spec, SpecName, SpecialSpec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, escape};

/// A completion option or action name (`-o nospace`, `-A arrayvar`), parsed by
/// its strum name. `CompleteAction` and `CompleteOption` live in brush-core,
/// which does not depend on winnow-args, so they cannot implement `FromArg`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Choice<T>(T);

impl<T> winnow_args::FromArg for Choice<T>
where
    T: std::str::FromStr + strum::VariantNames + strum::EnumMessage,
{
    const CHOICES: &'static [&'static str] = T::VARIANTS;
    const DESCRIBES_CHOICES: bool = true;

    /// The text written in `choices(…)`, which the pages show too, else the
    /// variant's doc comment in brush-core, without its final period.
    fn choice_help(index: usize, default: &'static str) -> &'static str {
        if !default.is_empty() {
            return default;
        }
        let variant = T::VARIANTS
            .get(index)
            .and_then(|name| name.parse::<T>().ok());
        let doc = variant.and_then(|variant| variant.get_documentation());
        doc.map_or("", |doc| doc.trim_end().trim_end_matches('.'))
    }

    fn from_arg(value: &winnow_args::BStr) -> Result<Self, winnow_args::error::BoxError> {
        std::str::from_utf8(value)
            .ok()
            .and_then(|name| name.parse().ok())
            .map(Choice)
            .ok_or_else(|| {
                Box::new(winnow_args::ChoiceError {
                    choices: T::VARIANTS,
                }) as winnow_args::error::BoxError
            })
    }
}

#[derive(winnow_args::Args)]
struct CommonCompleteCommandArgs {
    /// Options governing the behavior of completions.
    // The values are stated for the generated documentation, which cannot
    // read them from the type; the tests check them against it.
    #[arg(
        short = 'o',
        choices(
            "bashdefault" = "Perform rest of default completions if no completions are generated",
            "default" = "Use default filename completion if no completions are generated",
            "dirnames" = "Treat completions as directory names",
            "filenames" = "Treat completions as filenames",
            "fullquote" = "Quote completions even if they aren't file names. Not yet implemented: it's accepted and shown, but has no effect",
            "noquote" = "Suppress default auto-quotation of completions",
            "nosort" = "Do not sort completions",
            "nospace" = "Do not append a trailing space to completions at the end of the input line",
            "plusdirs" = "Also generate directory completions"
        )
    )]
    options: Vec<Choice<CompleteOption>>,

    /// Actions to apply to generate completions.
    #[arg(
        short = 'A',
        choices(
            "alias" = "Complete with valid aliases",
            "arrayvar" = "Complete with names of array shell variables",
            "binding" = "Complete with names of key bindings",
            "builtin" = "Complete with names of shell builtins",
            "command" = "Complete with names of executable commands",
            "directory" = "Complete with directory names",
            "disabled" = "Complete with names of disabled shell builtins",
            "enabled" = "Complete with names of enabled shell builtins",
            "export" = "Complete with names of exported shell variables",
            "file" = "Complete with filenames",
            "function" = "Complete with names of shell functions",
            "group" = "Complete with valid user groups",
            "helptopic" = "Complete with names of valid shell help topics",
            "hostname" = "Complete with the system's hostname(s)",
            "job" = "Complete with the command names of shell-managed jobs",
            "keyword" = "Complete with valid shell keywords",
            "running" = "Complete with the command names of running shell-managed jobs",
            "service" = "Complete with names of system services",
            "setopt" = "Complete with the names of options settable via set -o",
            "shopt" = "Complete with the names of options settable via shopt",
            "signal" = "Complete with the names of trappable signals",
            "stopped" = "Complete with the command names of stopped shell-managed jobs",
            "user" = "Complete with valid usernames",
            "variable" = "Complete with names of shell variables"
        )
    )]
    actions: Vec<Choice<CompleteAction>>,

    /// File glob pattern to be expanded to generate completions.
    #[arg(short = 'G', allow_hyphen_values, value_name = "GLOB")]
    glob_pattern: Option<String>,

    /// List of words that will be considered as completions.
    #[arg(short = 'W', allow_hyphen_values)]
    word_list: Option<String>,

    /// Name of a shell function to invoke to generate completions.
    #[arg(short = 'F', allow_hyphen_values, value_name = "FUNC_NAME")]
    function_name: Option<String>,

    /// Command to execute to generate completions.
    #[arg(short = 'C', allow_hyphen_values)]
    command: Option<String>,

    /// Pattern used as filter for completions.
    #[arg(short = 'X', allow_hyphen_values, value_name = "PATTERN")]
    filter_pattern: Option<String>,

    /// Prefix pattern used as filter for completions.
    #[arg(short = 'P', allow_hyphen_values)]
    prefix: Option<String>,

    /// Suffix pattern used as filter for completions.
    #[arg(short = 'S', allow_hyphen_values)]
    suffix: Option<String>,

    /// Complete with valid aliases.
    #[arg(short = 'a')]
    action_alias: bool,

    /// Complete with names of shell builtins.
    #[arg(short = 'b')]
    action_builtin: bool,

    /// Complete with names of executable commands.
    #[arg(short = 'c')]
    action_command: bool,

    /// Complete with directory names.
    #[arg(short = 'd')]
    action_directory: bool,

    /// Complete with names of exported shell variables.
    #[arg(short = 'e')]
    action_exported: bool,

    /// Complete with filenames.
    #[arg(short = 'f')]
    action_file: bool,

    /// Complete with valid user groups.
    #[arg(short = 'g')]
    action_group: bool,

    /// Complete with the command names of shell-managed jobs.
    #[arg(short = 'j')]
    action_job: bool,

    /// Complete with valid shell keywords.
    #[arg(short = 'k')]
    action_keyword: bool,

    /// Complete with names of system services.
    #[arg(short = 's')]
    action_service: bool,

    /// Complete with valid usernames.
    #[arg(short = 'u')]
    action_user: bool,

    /// Complete with names of shell variables.
    #[arg(short = 'v')]
    action_variable: bool,
}

/// The actions with flags of their own in `complete` and `compgen`, and those flags (e.g.
/// `a`, for `-a`, which is short for `-A alias`), in the order bash shows them.
const ACTION_FLAGS: [(char, CompleteAction); 12] = [
    ('a', CompleteAction::Alias),
    ('b', CompleteAction::Builtin),
    ('c', CompleteAction::Command),
    ('d', CompleteAction::Directory),
    ('e', CompleteAction::Export),
    ('f', CompleteAction::File),
    ('g', CompleteAction::Group),
    ('j', CompleteAction::Job),
    ('k', CompleteAction::Keyword),
    ('s', CompleteAction::Service),
    ('u', CompleteAction::User),
    ('v', CompleteAction::Variable),
];

impl CommonCompleteCommandArgs {
    fn create_spec(&self) -> completion::Spec {
        completion::Spec {
            options: self.options.iter().map(|o| o.0).collect(),
            actions: self.resolve_actions(),
            glob_pattern: self.glob_pattern.clone(),
            word_list: self.word_list.clone(),
            function_name: self.function_name.clone(),
            command: self.command.clone(),
            filter_pattern: self.filter_pattern.clone(),
            prefix: self.prefix.clone(),
            suffix: self.suffix.clone(),
        }
    }

    /// Returns the actions selected, like bash, each once, in the fixed order bash runs them
    /// in (that of [`CompleteAction`]'s variants), however they were given.
    fn resolve_actions(&self) -> Vec<CompleteAction> {
        // In the order of `ACTION_FLAGS`.
        let flags = [
            self.action_alias,
            self.action_builtin,
            self.action_command,
            self.action_directory,
            self.action_exported,
            self.action_file,
            self.action_group,
            self.action_job,
            self.action_keyword,
            self.action_service,
            self.action_user,
            self.action_variable,
        ];
        let selected: Vec<CompleteAction> = self
            .actions
            .iter()
            .map(|a| a.0)
            .chain(
                ACTION_FLAGS
                    .iter()
                    .zip(flags)
                    .filter_map(|((_, action), given)| given.then_some(*action)),
            )
            .collect();
        CompleteAction::iter()
            .filter(|action| selected.contains(action))
            .collect()
    }
}

/// Configure programmable command completion.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct CompleteCommand {
    /// Display registered completion settings.
    #[arg(short = 'p')]
    print: bool,

    /// Remove the completion settings associated with the given command.
    #[arg(short = 'r')]
    remove: bool,

    /// Apply these settings to the default completion scenario.
    #[arg(short = 'D')]
    use_as_default: bool,

    /// Apply these settings to completion of empty lines.
    #[arg(short = 'E')]
    use_for_empty_line: bool,

    /// Apply these settings to completion of the initial word of the input line.
    #[arg(short = 'I')]
    use_for_initial_word: bool,

    #[arg(flatten)]
    common_args: CommonCompleteCommandArgs,

    #[arg(positional)]
    names: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(CompleteCommand);

/// Returns the flag that selects a special completion spec in `complete` and `compopt`.
const fn special_spec_flag(special: SpecialSpec) -> &'static str {
    match special {
        SpecialSpec::Default => "-D",
        SpecialSpec::EmptyLine => "-E",
        SpecialSpec::InitialWord => "-I",
    }
}

/// Returns the operand that names the spec `name` in a `complete` or `compopt` command that
/// `complete -p` or `compopt` prints: a special spec's flag, or the command's name, quoted
/// if needed.
fn spec_name_operand(name: SpecName<'_>) -> Cow<'_, str> {
    match name {
        SpecName::Special(special) => special_spec_flag(special).into(),
        SpecName::Command(command) => {
            escape::quote_if_needed(command, escape::QuoteMode::SingleQuote)
        }
    }
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for CompleteCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // Like bash, -D, -E, or -I name a special spec in place of any names given; they
        // take precedence in that order.
        let special = [
            (self.use_as_default, SpecialSpec::Default),
            (self.use_for_empty_line, SpecialSpec::EmptyLine),
            (self.use_for_initial_word, SpecialSpec::InitialWord),
        ]
        .into_iter()
        .find_map(|(selected, special)| selected.then_some(special));
        let names: Vec<SpecName<'_>> = match special {
            Some(special) => vec![SpecName::Special(special)],
            None => self
                .names
                .iter()
                .map(|name| SpecName::parse(name))
                .collect(),
        };

        // With no spec named, list them all (as `complete` with no options does too), or
        // with `-r`, remove them all.
        if names.is_empty() {
            if self.remove && !self.print {
                context.shell.completion_config_mut().clear();
            } else {
                // Sort, so the listing is stable; the special specs come last.
                let mut specs: Vec<_> = context.shell.completion_config().iter().collect();
                specs.sort_by_key(|(name, _)| *name);
                for (name, spec) in specs {
                    Self::display_spec(&context, name, spec)?;
                }
            }
            return Ok(ExecutionResult::success());
        }

        let mut result = ExecutionResult::success();
        for name in names {
            if !self.try_process_for_command(&mut context, name)? {
                result = ExecutionResult::general_error();
            }
        }

        Ok(result)
    }
}

impl CompleteCommand {
    fn try_display_spec_for_command(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: SpecName<'_>,
    ) -> Result<bool, brush_core::Error> {
        if let Some(spec) = context.shell.completion_config().get(name) {
            Self::display_spec(context, name, spec)?;
            Ok(true)
        } else {
            writeln!(
                context.stderr(),
                "complete: {}: no completion specification",
                name.as_str()
            )?;
            Ok(false)
        }
    }

    /// Displays `spec`, registered under `name`, as the `complete` command that recreates
    /// it, formatted as bash does.
    fn display_spec(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: SpecName<'_>,
        spec: &Spec,
    ) -> Result<(), brush_core::Error> {
        let mut s = String::from("complete");

        // Options, in the order bash shows them.
        let options = &spec.options;
        for option in CompleteOption::iter().filter(|option| options.get(*option)) {
            write!(s, " -o {}", <&str>::from(option))?;
        }

        // Like bash, show each action once: those with their own flag first, then the rest
        // with `-A`, each in the order bash lists them.
        for (flag, action) in &ACTION_FLAGS {
            if spec.actions.contains(action) {
                write!(s, " -{flag}")?;
            }
        }
        for action in CompleteAction::iter() {
            if spec.actions.contains(&action) && !ACTION_FLAGS.iter().any(|(_, a)| *a == action) {
                write!(s, " -A {}", <&str>::from(action))?;
            }
        }

        for (flag, arg) in [
            ("-G", spec.glob_pattern.as_ref()),
            ("-W", spec.word_list.as_ref()),
            ("-P", spec.prefix.as_ref()),
            ("-S", spec.suffix.as_ref()),
            ("-X", spec.filter_pattern.as_ref()),
            ("-C", spec.command.as_ref()),
        ] {
            if let Some(arg) = arg {
                let arg = escape::force_quote(arg, escape::QuoteMode::SingleQuote);
                write!(s, " {flag} {arg}")?;
            }
        }
        if let Some(function_name) = &spec.function_name {
            let function_name =
                escape::quote_if_needed(function_name, escape::QuoteMode::SingleQuote);
            write!(s, " -F {function_name}")?;
        }

        // Like bash, the spec's name comes last.
        writeln!(context.stdout(), "{s} {}", spec_name_operand(name))?;

        Ok(())
    }

    fn try_process_for_command(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: SpecName<'_>,
    ) -> Result<bool, brush_core::Error> {
        if self.print {
            return Self::try_display_spec_for_command(context, name);
        } else if self.remove {
            let mut result = context.shell.completion_config_mut().remove(name);

            if !result {
                if context.shell.options().interactive {
                    writeln!(context.stderr(), "complete: {}: not found", name.as_str())?;
                } else {
                    // For some reason, this is not supposed to be treated as a failure
                    // in non-interactive execution.
                    result = true;
                }
            }

            return Ok(result);
        }

        let config = self.common_args.create_spec();

        context.shell.completion_config_mut().set(name, config);

        Ok(true)
    }
}

/// Generate command completions.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct CompGenCommand {
    #[arg(flatten)]
    common_args: CommonCompleteCommandArgs,

    // N.B. The word can only start with a hyphen if it's after a --.
    #[arg(positional)]
    word: Option<String>,
}

brush_winnow_args_adapter::impl_parse_args!(CompGenCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for CompGenCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let spec = self.common_args.create_spec();
        let word = self.word.as_deref().unwrap_or_default();
        let candidates = spec.generate(context.shell, word).await?;

        // We are expected to return 1 if there are no candidates, even if no errors
        // occurred along the way.
        if candidates.is_empty() {
            return Ok(ExecutionResult::general_error());
        }

        for candidate in candidates {
            writeln!(context.stdout(), "{candidate}")?;
        }

        Ok(ExecutionResult::success())
    }
}

/// Set programmable command completion options.
#[derive(winnow_args::Args)]
#[arg(
    plus_options,
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct CompOptCommand {
    /// Update the default completion settings.
    #[arg(short = 'D')]
    update_default: bool,

    /// Update the completion settings for empty lines.
    #[arg(short = 'E')]
    update_empty: bool,

    /// Update the completion settings for the initial word of the input line.
    #[arg(short = 'I')]
    update_initial_word: bool,

    /// Enable the specified option for selected completion scenarios.
    #[arg(
        short = 'o',
        value_name = "OPT",
        choices(
            "bashdefault" = "Perform rest of default completions if no completions are generated",
            "default" = "Use default filename completion if no completions are generated",
            "dirnames" = "Treat completions as directory names",
            "filenames" = "Treat completions as filenames",
            "fullquote" = "Quote completions even if they aren't file names. Not yet implemented: it's accepted and shown, but has no effect",
            "noquote" = "Suppress default auto-quotation of completions",
            "nosort" = "Do not sort completions",
            "nospace" = "Do not append a trailing space to completions at the end of the input line",
            "plusdirs" = "Also generate directory completions"
        )
    )]
    enabled_options: Vec<Choice<CompleteOption>>,

    /// Disable the specified option for selected completion scenarios.
    #[arg(
        plus = 'o',
        value_name = "OPT",
        choices(
            "bashdefault" = "Perform rest of default completions if no completions are generated",
            "default" = "Use default filename completion if no completions are generated",
            "dirnames" = "Treat completions as directory names",
            "filenames" = "Treat completions as filenames",
            "fullquote" = "Quote completions even if they aren't file names. Not yet implemented: it's accepted and shown, but has no effect",
            "noquote" = "Suppress default auto-quotation of completions",
            "nosort" = "Do not sort completions",
            "nospace" = "Do not append a trailing space to completions at the end of the input line",
            "plusdirs" = "Also generate directory completions"
        )
    )]
    disabled_options: Vec<Choice<CompleteOption>>,

    /// If specified, scopes updates to completions of the named commands.
    #[arg(positional)]
    names: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(CompOptCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for CompOptCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // Like bash, an option given with both `-o` and `+o` is disabled, whatever their
        // order.
        let mut options =
            HashMap::with_capacity(self.disabled_options.len() + self.enabled_options.len());
        for option in &self.enabled_options {
            options.insert(option.0, true);
        }
        for option in &self.disabled_options {
            options.insert(option.0, false);
        }

        if !self.names.is_empty()
            && (self.update_default || self.update_empty || self.update_initial_word)
        {
            writeln!(
                context.stderr(),
                "compopt: cannot specify names with -D, -E, or -I"
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        // -D, -E, and -I select a special spec, which, like bash, messages name by bash's name
        // for it.
        let special = [
            (self.update_default, SpecialSpec::Default),
            (self.update_empty, SpecialSpec::EmptyLine),
            (self.update_initial_word, SpecialSpec::InitialWord),
        ]
        .into_iter()
        .find_map(|(selected, special)| selected.then_some(special));
        let names: Vec<SpecName<'_>> = match special {
            Some(special) => vec![SpecName::Special(special)],
            None => self
                .names
                .iter()
                .map(|name| SpecName::parse(name))
                .collect(),
        };

        if !names.is_empty() {
            // Like bash, a spec that doesn't exist is an error, rather than being created.
            // With no options to set, each spec's options are shown instead.
            let mut result = ExecutionResult::success();
            for name in names {
                let found = if options.is_empty() {
                    match context.shell.completion_config().get(name) {
                        Some(spec) => {
                            Self::display_options(&context, name, &spec.options)?;
                            true
                        }
                        None => false,
                    }
                } else if let Some(spec) = context.shell.completion_config_mut().get_mut(name) {
                    Self::set_options_for_spec(spec, &options);
                    true
                } else {
                    false
                };
                if !found {
                    writeln!(
                        context.stderr(),
                        "compopt: {}: no completion specification",
                        name.as_str()
                    )?;
                    result = ExecutionResult::general_error();
                }
            }
            return Ok(result);
        }

        // With no names, apply to the completion in progress, if there is one, or with no
        // options to set, show its options.
        if options.is_empty() {
            if let Some(in_progress) = context.shell.in_progress_completion() {
                Self::display_options(&context, in_progress.spec_name(), in_progress.options())?;
                return Ok(ExecutionResult::success());
            }
        } else if let Some(in_progress_options) = context.shell.in_progress_completion_options_mut()
        {
            Self::set_options(in_progress_options, &options);
            return Ok(ExecutionResult::success());
        }

        writeln!(
            context.stderr(),
            "compopt: not currently executing completion function"
        )?;
        Ok(ExecutionResult::general_error())
    }
}

impl CompOptCommand {
    /// Displays `options`, those of the spec `name`, as the `compopt` command that sets
    /// them: each option enabled (`-o`) or disabled (`+o`), formatted as bash does.
    fn display_options(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: SpecName<'_>,
        options: &completion::GenerationOptions,
    ) -> Result<(), brush_core::Error> {
        let mut s = String::from("compopt");
        for option in CompleteOption::iter() {
            let sign = if options.get(option) { '-' } else { '+' };
            write!(s, " {sign}o {}", <&str>::from(option))?;
        }
        writeln!(context.stdout(), "{s} {}", spec_name_operand(name))?;

        Ok(())
    }

    fn set_options_for_spec<'a, I>(spec: &mut Spec, options: I)
    where
        I: IntoIterator<Item = (&'a CompleteOption, &'a bool)>,
    {
        Self::set_options(&mut spec.options, options);
    }

    fn set_options<'a, I>(target_options: &mut completion::GenerationOptions, options: I)
    where
        I: IntoIterator<Item = (&'a CompleteOption, &'a bool)>,
    {
        for (option, value) in options {
            target_options.set(*option, *value);
        }
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;
    use brush_core::builtins::{Invocation, ParseArgs, UsageError};
    use strum::VariantNames;

    fn parse<T: ParseArgs>(name: &str, words: [&str; 2]) -> Result<T, UsageError> {
        T::parse(Invocation::new(name, words))
    }

    #[test]
    fn every_action_name_parses_to_its_action() -> Result<()> {
        for name in CompleteAction::VARIANTS {
            let args = parse::<CompleteCommand>("complete", ["-A", name])?;
            let parsed: Vec<_> = args
                .common_args
                .actions
                .iter()
                .map(|a| <&str>::from(a.0))
                .collect();
            assert_eq!(parsed, [*name]);
        }
        Ok(())
    }

    /// What `choices(…)` says each value means, which the pages show, is
    /// what brush-core's doc comment says, which help would show without it.
    #[test]
    fn each_choice_says_what_its_docs_say() {
        use winnow_args::FromArg;
        let items = <CompleteCommand as winnow_args::Args>::HELP.items.iter();
        let items = items.chain(<CompOptCommand as winnow_args::Args>::HELP.items);
        let mut checked = 0;
        for item in items.filter(|item| !item.choices.is_empty()) {
            // Without a function, each says nothing, and the docs disagree.
            let help = item.choice_help.unwrap_or(|_| "");
            for (index, choice) in item.choices.iter().enumerate() {
                let from_docs = if item.choices[0] == "alias" {
                    Choice::<CompleteAction>::choice_help(index, "")
                } else {
                    Choice::<CompleteOption>::choice_help(index, "")
                };
                let from_docs = from_docs.split_whitespace().collect::<Vec<_>>().join(" ");
                assert_eq!(help(index), from_docs, "{choice}");
            }
            checked += 1;
        }
        assert_eq!(checked, 4);
    }

    #[test]
    fn every_option_name_parses_to_its_option() -> Result<()> {
        for name in CompleteOption::VARIANTS {
            let option: CompleteOption = name.parse()?;
            let args = parse::<CompleteCommand>("complete", ["-o", name])?;
            assert_eq!(args.common_args.options, [Choice(option)]);

            let args = parse::<CompOptCommand>("compopt", ["+o", name])?;
            assert_eq!(args.disabled_options, [Choice(option)]);
        }
        Ok(())
    }

    #[test]
    fn invalid_names_list_the_accepted_ones() {
        let Err(err) = parse::<CompleteCommand>("complete", ["-A", "bogus"]) else {
            unreachable!("`bogus` is not an action name");
        };
        let message = err.to_string();
        assert!(
            message.contains("bogus") && message.contains("arrayvar"),
            "{message}"
        );
    }

    #[test]
    fn help_lists_accepted_names() {
        let help = <CompleteCommand as ParseArgs>::content(
            "complete",
            builtins::ContentType::DetailedHelp,
            &builtins::ContentOptions::default(),
        )
        .unwrap_or_default();
        assert!(
            help.contains("arrayvar") && help.contains("nospace"),
            "{help}"
        );
    }
}
