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

impl<T: std::str::FromStr + strum::VariantNames> winnow_args::FromArg for Choice<T> {
    const CHOICES: &'static [&'static str] = T::VARIANTS;

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
    // read them from the type; a documentation build checks them against it.
    #[arg(
        short = 'o',
        choices(
            "bashdefault",
            "default",
            "dirnames",
            "filenames",
            "noquote",
            "nosort",
            "nospace",
            "plusdirs"
        )
    )]
    options: Vec<Choice<CompleteOption>>,

    /// Actions to apply to generate completions.
    #[arg(
        short = 'A',
        choices(
            "alias",
            "arrayvar",
            "binding",
            "builtin",
            "command",
            "directory",
            "disabled",
            "enabled",
            "export",
            "file",
            "function",
            "group",
            "helptopic",
            "hostname",
            "job",
            "keyword",
            "running",
            "service",
            "setopt",
            "shopt",
            "signal",
            "stopped",
            "user",
            "variable"
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

    /// Complete with job specs.
    #[arg(short = 'j')]
    action_job: bool,

    /// Complete with keywords.
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

brush_builtin_winnow::winnow_builtin!(CompleteCommand);

/// Returns the flag that selects a special completion spec in `complete` and `compopt`.
const fn special_spec_flag(special: SpecialSpec) -> &'static str {
    match special {
        SpecialSpec::Default => "-D",
        SpecialSpec::EmptyLine => "-E",
        SpecialSpec::InitialWord => "-I",
    }
}

impl builtins::Command for CompleteCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
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
                    Self::display_named_spec(&context, name, spec)?;
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
    /// Displays `spec`, registered under `name`; a special spec is shown by the flag that
    /// selects it.
    fn display_named_spec(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: SpecName<'_>,
        spec: &Spec,
    ) -> Result<(), brush_core::Error> {
        match name {
            SpecName::Special(special) => {
                Self::display_spec(context, Some(special_spec_flag(special)), None, spec)
            }
            SpecName::Command(command) => Self::display_spec(context, None, Some(command), spec),
        }
    }

    fn try_display_spec_for_command(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: SpecName<'_>,
    ) -> Result<bool, brush_core::Error> {
        if let Some(spec) = context.shell.completion_config().get(name) {
            Self::display_named_spec(context, name, spec)?;
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

    /// Displays `spec` as the `complete` command that recreates it, formatted as bash does.
    fn display_spec(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        special_name: Option<&str>,
        command_name: Option<&str>,
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

        // Like bash, the spec's name comes last: a special spec's flag, or the command's
        // name, quoted if needed.
        let name = special_name.map_or_else(
            || {
                escape::quote_if_needed(
                    command_name.unwrap_or_default(),
                    escape::QuoteMode::SingleQuote,
                )
            },
            Into::into,
        );
        writeln!(context.stdout(), "{s} {name}")?;

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

brush_builtin_winnow::winnow_builtin!(CompGenCommand);

impl builtins::Command for CompGenCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
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
            "bashdefault",
            "default",
            "dirnames",
            "filenames",
            "noquote",
            "nosort",
            "nospace",
            "plusdirs"
        )
    )]
    enabled_options: Vec<Choice<CompleteOption>>,

    #[arg(
        plus = 'o',
        value_name = "OPT",
        choices(
            "bashdefault",
            "default",
            "dirnames",
            "filenames",
            "noquote",
            "nosort",
            "nospace",
            "plusdirs"
        )
    )]
    disabled_options: Vec<Choice<CompleteOption>>,

    /// If specified, scopes updates to completions of the named commands.
    #[arg(positional)]
    names: Vec<String>,
}

brush_builtin_winnow::winnow_builtin!(CompOptCommand);

impl builtins::Command for CompOptCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
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
            let mut result = ExecutionResult::success();
            for name in names {
                if let Some(spec) = context.shell.completion_config_mut().get_mut(name) {
                    Self::set_options_for_spec(spec, &options);
                } else {
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

        // With no names, apply to the completion in progress, if there is one.
        let Some(in_progress_options) = context.shell.in_progress_completion_options_mut() else {
            writeln!(
                context.stderr(),
                "compopt: not currently executing completion function"
            )?;
            return Ok(ExecutionResult::general_error());
        };
        Self::set_options(in_progress_options, &options);

        Ok(ExecutionResult::success())
    }
}

impl CompOptCommand {
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
    use brush_core::CommandArg;
    use brush_core::builtins::FromArgs;
    use strum::VariantNames;

    fn arg(word: &str) -> CommandArg {
        CommandArg::String(word.to_owned())
    }

    #[test]
    fn every_action_name_parses_to_its_action() -> Result<()> {
        for name in CompleteAction::VARIANTS {
            let args = CompleteCommand::from_args("complete", vec![arg("-A"), arg(name)])?;
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

    #[test]
    fn every_option_name_parses_to_its_option() -> Result<()> {
        for name in CompleteOption::VARIANTS {
            let option: CompleteOption = name.parse()?;
            let args = CompleteCommand::from_args("complete", vec![arg("-o"), arg(name)])?;
            assert_eq!(args.common_args.options, [Choice(option)]);

            let args = CompOptCommand::from_args("compopt", vec![arg("+o"), arg(name)])?;
            assert_eq!(args.disabled_options, [Choice(option)]);
        }
        Ok(())
    }

    #[test]
    fn invalid_names_list_the_accepted_ones() {
        let Err(err) = CompleteCommand::from_args("complete", vec![arg("-A"), arg("bogus")]) else {
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
        let help = <CompleteCommand as builtins::HelpContent>::detailed_help(
            "complete",
            &builtins::ContentOptions::default(),
        )
        .unwrap_or_default();
        assert!(
            help.contains("arrayvar") && help.contains("nospace"),
            "{help}"
        );
    }
}
