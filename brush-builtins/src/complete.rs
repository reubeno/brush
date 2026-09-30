use clap::{Parser, builder::TypedValueParser as _};
use itertools::Itertools;
use std::fmt::Write as _;
use std::io::Write;
use strum::{EnumMessage, IntoEnumIterator};

use brush_core::completion::{self, CompleteAction, CompleteOption, Spec, SpecName, SpecialSpec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, escape};

/// Returns the flag that selects a special completion spec in `complete` and `compopt`.
const fn special_spec_flag(special: SpecialSpec) -> &'static str {
    match special {
        SpecialSpec::Default => "-D",
        SpecialSpec::EmptyLine => "-E",
        SpecialSpec::InitialWord => "-I",
    }
}

/// Returns the special spec that `-D`, `-E`, or `-I` selects, if one is given; they take
/// precedence in that order.
const fn selected_special_spec(
    default: bool,
    empty_line: bool,
    initial_word: bool,
) -> Option<SpecialSpec> {
    if default {
        Some(SpecialSpec::Default)
    } else if empty_line {
        Some(SpecialSpec::EmptyLine)
    } else if initial_word {
        Some(SpecialSpec::InitialWord)
    } else {
        None
    }
}

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

    fn resolve_actions(&self) -> Vec<CompleteAction> {
        self.actions
            .iter()
            .chain(&self.action_flags.0)
            .copied()
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

impl builtins::Command for CompleteCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // Like bash, -D, -E, or -I name a special spec in place of any names given.
        let special = selected_special_spec(
            self.use_as_default,
            self.use_for_empty_line,
            self.use_for_initial_word,
        );
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
                // Sort, so the listing is stable.
                for (name, spec) in context
                    .shell
                    .completion_config()
                    .iter()
                    .sorted_by_key(|(name, _)| *name)
                {
                    Self::display_spec(&context, name, spec)?;
                }
            }
            return Ok(ExecutionResult::success());
        }

        let mut result = ExecutionResult::success();
        for name in names {
            if !self.process_spec(&mut context, name)? {
                result = ExecutionResult::general_error();
            }
        }

        Ok(result)
    }
}

impl CompleteCommand {
    /// Displays the spec `name`, if there is one; otherwise, reports that there isn't and
    /// returns false.
    fn try_display_spec(
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

        // The options are declared in the order bash shows them.
        for option in CompleteOption::iter().filter(|option| spec.options.get(*option)) {
            write!(s, " -o {}", <&str>::from(option))?;
        }

        // Like bash, show each action once: those with their own flag first, then the rest
        // with `-A`.
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

        let name = match name {
            SpecName::Command(command) => {
                escape::quote_if_needed(command, escape::QuoteMode::SingleQuote)
            }
            SpecName::Special(special) => special_spec_flag(special).into(),
        };
        writeln!(context.stdout(), "{s} {name}")?;

        Ok(())
    }

    /// Displays, removes, or sets the spec `name`, as the command's options say; returns
    /// whether that succeeded.
    fn process_spec(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: SpecName<'_>,
    ) -> Result<bool, brush_core::Error> {
        if self.print {
            Self::try_display_spec(context, name)
        } else if self.remove {
            if context.shell.completion_config_mut().remove(name) {
                Ok(true)
            } else if context.shell.options().interactive {
                writeln!(context.stderr(), "complete: {}: not found", name.as_str())?;
                Ok(false)
            } else {
                // For some reason, this is not supposed to be treated as a failure in
                // non-interactive execution.
                Ok(true)
            }
        } else {
            let spec = self.common_args.create_spec();
            context.shell.completion_config_mut().set(name, spec);
            Ok(true)
        }
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
        let special = selected_special_spec(
            self.update_default,
            self.update_empty,
            self.update_initial_word,
        );

        let names: Vec<SpecName<'_>> = match special {
            Some(_) if !self.names.is_empty() => {
                writeln!(
                    context.stderr(),
                    "compopt: cannot specify names with -D, -E, or -I"
                )?;
                return Ok(ExecutionExitCode::InvalidUsage.into());
            }
            Some(special) => vec![SpecName::Special(special)],
            None => self
                .names
                .iter()
                .map(|name| SpecName::parse(name))
                .collect(),
        };

        // With no names, apply to the completion in progress, if there is one.
        if names.is_empty() {
            let Some(in_progress_options) = context.shell.in_progress_completion_options_mut()
            else {
                writeln!(
                    context.stderr(),
                    "compopt: not currently executing completion function"
                )?;
                return Ok(ExecutionResult::general_error());
            };
            self.set_options(in_progress_options);
            return Ok(ExecutionResult::success());
        }

        let mut result = ExecutionResult::success();
        for name in names {
            if let Some(spec) = context.shell.completion_config_mut().get_mut(name) {
                self.set_options(&mut spec.options);
            } else {
                writeln!(
                    context.stderr(),
                    "compopt: {}: no completion specification",
                    name.as_str()
                )?;
                result = ExecutionResult::general_error();
            }
        }

        Ok(result)
    }
}

impl CompOptCommand {
    /// Applies the options given to `options`: disabling those given with `+o`, then
    /// enabling those given with `-o`, which win.
    fn set_options(&self, options: &mut completion::GenerationOptions) {
        for option in &self.disabled_options {
            options.set(*option, false);
        }
        for option in &self.enabled_options {
            options.set(*option, true);
        }
    }
}
