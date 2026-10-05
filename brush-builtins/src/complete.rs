use clap::{Parser, builder::TypedValueParser as _};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use strum::{EnumMessage, IntoEnumIterator};

use brush_core::completion::{self, CompleteAction, CompleteOption, Spec, SpecName, SpecialSpec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, escape};

#[derive(Parser)]
struct CommonCompleteCommandArgs {
    /// Options governing the behavior of completions.
    #[arg(short = 'o', value_parser = name_parser::<CompleteOption>())]
    options: Vec<CompleteOption>,

    /// Actions to apply to generate completions.
    #[arg(short = 'A', value_parser = name_parser::<CompleteAction>())]
    actions: Vec<CompleteAction>,

    /// File glob pattern to be expanded to generate completions.
    #[arg(short = 'G', allow_hyphen_values = true, value_name = "GLOB")]
    glob_pattern: Option<String>,

    /// List of words that will be considered as completions.
    #[arg(short = 'W', allow_hyphen_values = true)]
    word_list: Option<String>,

    /// Name of a shell function to invoke to generate completions.
    #[arg(short = 'F', allow_hyphen_values = true, value_name = "FUNC_NAME")]
    function_name: Option<String>,

    /// Command to execute to generate completions.
    #[arg(short = 'C', allow_hyphen_values = true)]
    command: Option<String>,

    /// Pattern used as filter for completions.
    #[arg(short = 'X', allow_hyphen_values = true, value_name = "PATTERN")]
    filter_pattern: Option<String>,

    /// Prefix pattern used as filter for completions.
    #[arg(short = 'P', allow_hyphen_values = true)]
    prefix: Option<String>,

    /// Suffix pattern used as filter for completions.
    #[arg(short = 'S', allow_hyphen_values = true)]
    suffix: Option<String>,

    /// Actions selected with their own flags (e.g. `-a`).
    #[clap(flatten)]
    action_flags: ActionFlags,
}

/// Returns a parser for the name of one of `E`'s variants (e.g. `alias` for
/// [`CompleteAction::Alias`]), which lists the names, and describes each with its docs.
fn name_parser<E>() -> impl clap::builder::TypedValueParser<Value = E>
where
    E: IntoEnumIterator + EnumMessage + std::str::FromStr + Clone + Send + Sync + 'static,
    &'static str: From<E>,
    E::Err: std::error::Error + Send + Sync + 'static,
{
    let names = E::iter().map(|variant| {
        let help = help_for(&variant);
        clap::builder::PossibleValue::new(<&'static str>::from(variant)).help(help)
    });
    clap::builder::PossibleValuesParser::new(names).try_map(|name| name.parse::<E>())
}

/// Returns the help for `variant`: its docs, without a trailing period, as clap shows help.
fn help_for(variant: &impl EnumMessage) -> &'static str {
    let docs = variant.get_documentation().unwrap_or_default();
    docs.strip_suffix('.').unwrap_or(docs)
}

/// The actions with flags of their own in `complete` and `compgen`, and those flags (e.g.
/// `a`, for `-a`, which is short for `-A alias`), in the order bash shows them. Each flag
/// is also its argument's clap ID.
const ACTION_FLAGS: [(&str, CompleteAction); 12] = [
    ("a", CompleteAction::Alias),
    ("b", CompleteAction::Builtin),
    ("c", CompleteAction::Command),
    ("d", CompleteAction::Directory),
    ("e", CompleteAction::Export),
    ("f", CompleteAction::File),
    ("g", CompleteAction::Group),
    ("j", CompleteAction::Job),
    ("k", CompleteAction::Keyword),
    ("s", CompleteAction::Service),
    ("u", CompleteAction::User),
    ("v", CompleteAction::Variable),
];

/// The actions selected with their own flags (see [`ACTION_FLAGS`]), which this parses.
struct ActionFlags(Vec<CompleteAction>);

impl clap::FromArgMatches for ActionFlags {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        let actions = ACTION_FLAGS
            .iter()
            .filter(|(flag, _)| matches.get_flag(flag))
            .map(|(_, action)| *action)
            .collect();
        Ok(Self(actions))
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl clap::Args for ActionFlags {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        ACTION_FLAGS.iter().fold(cmd, |cmd, (flag, action)| {
            cmd.arg(
                clap::Arg::new(*flag)
                    .short(flag.chars().next())
                    .action(clap::ArgAction::SetTrue)
                    .help(help_for(action)),
            )
        })
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        Self::augment_args(cmd)
    }
}

impl CommonCompleteCommandArgs {
    fn create_spec(&self) -> completion::Spec {
        completion::Spec {
            options: self.options.iter().copied().collect(),
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
        let selected: Vec<_> = self.actions.iter().chain(&self.action_flags.0).collect();
        CompleteAction::iter()
            .filter(|action| selected.contains(&action))
            .collect()
    }
}

/// Configure programmable command completion.
#[derive(Parser)]
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

    #[clap(flatten)]
    common_args: CommonCompleteCommandArgs,

    names: Vec<String>,
}

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
#[derive(Parser)]
pub(crate) struct CompGenCommand {
    #[clap(flatten)]
    common_args: CommonCompleteCommandArgs,

    // N.B. The word can only start with a hyphen if it's after a --.
    word: Option<String>,
}

impl builtins::Command for CompGenCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut spec = self.common_args.create_spec();
        spec.options.set(CompleteOption::NoSort, true);

        let token_to_complete = self.word.as_deref().unwrap_or_default();

        // We unquote the token-to-be-completed before passing it to the completion system.
        let unquoted_token = brush_parser::unquote_str(token_to_complete);

        let completion_context = completion::Context {
            token_to_complete: unquoted_token.as_str(),
            preceding_token: None,
            command_name: None,
            token_index: 0,
            tokens: &[&completion::CompletionToken {
                text: token_to_complete,
                start: 0,
            }],
            input_line: token_to_complete,
            cursor_index: token_to_complete.len(),
            trigger: completion::CompletionTrigger::Programmatic,
        };

        let result = spec
            .get_completions(context.shell, &completion_context)
            .await?;

        match result {
            completion::Answer::Candidates(candidates, _options) => {
                // We are expected to return 1 if there are no candidates, even if no errors
                // occurred along the way.
                if candidates.is_empty() {
                    return Ok(ExecutionResult::general_error());
                }

                for candidate in candidates {
                    writeln!(context.stdout(), "{candidate}")?;
                }
            }
            completion::Answer::RestartCompletionProcess => {
                return error::unimp("restart completion");
            }
        }

        Ok(ExecutionResult::success())
    }
}

/// Set programmable command completion options.
#[derive(Parser)]
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
    #[arg(short = 'o', value_name = "OPT", value_parser = name_parser::<CompleteOption>())]
    enabled_options: Vec<CompleteOption>,
    #[arg(long = "+o", hide = true, value_parser = name_parser::<CompleteOption>())]
    disabled_options: Vec<CompleteOption>,

    /// If specified, scopes updates to completions of the named commands.
    names: Vec<String>,
}

impl builtins::Command for CompOptCommand {
    type Error = brush_core::Error;

    fn takes_plus_options() -> bool {
        true
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // Like bash, an option given with both `-o` and `+o` is disabled, whatever their
        // order.
        let mut options =
            HashMap::with_capacity(self.disabled_options.len() + self.enabled_options.len());
        for option in &self.enabled_options {
            options.insert(*option, true);
        }
        for option in &self.disabled_options {
            options.insert(*option, false);
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
