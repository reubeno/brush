use clap::{Parser, ValueEnum as _};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use strum::IntoEnumIterator;

use brush_core::completion::{self, CompleteAction, CompleteOption, Spec, SpecialSpec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, escape};

#[derive(Parser)]
struct CommonCompleteCommandArgs {
    /// Options governing the behavior of completions.
    #[arg(short = 'o')]
    options: Vec<CompleteOption>,

    /// Actions to apply to generate completions.
    #[arg(short = 'A')]
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

impl CommonCompleteCommandArgs {
    fn create_spec(&self) -> completion::Spec {
        let mut spec = completion::Spec {
            options: completion::GenerationOptions::default(),
            actions: self.resolve_actions(),
            glob_pattern: self.glob_pattern.clone(),
            word_list: self.word_list.clone(),
            function_name: self.function_name.clone(),
            command: self.command.clone(),
            filter_pattern: self.filter_pattern.clone(),
            prefix: self.prefix.clone(),
            suffix: self.suffix.clone(),
        };

        for option in &self.options {
            match option {
                CompleteOption::BashDefault => spec.options.bash_default = true,
                CompleteOption::Default => spec.options.default = true,
                CompleteOption::DirNames => spec.options.dir_names = true,
                CompleteOption::FileNames => spec.options.file_names = true,
                CompleteOption::NoQuote => spec.options.no_quote = true,
                CompleteOption::NoSort => spec.options.no_sort = true,
                CompleteOption::NoSpace => spec.options.no_space = true,
                CompleteOption::PlusDirs => spec.options.plus_dirs = true,
            }
        }

        spec
    }

    /// Returns the actions selected, like bash, each once, in the fixed order bash runs them
    /// in (that of [`CompleteAction`]'s variants), however they were given.
    fn resolve_actions(&self) -> Vec<CompleteAction> {
        let mut actions = self.actions.clone();

        actions.extend(
            [
                (self.action_alias, CompleteAction::Alias),
                (self.action_builtin, CompleteAction::Builtin),
                (self.action_command, CompleteAction::Command),
                (self.action_directory, CompleteAction::Directory),
                (self.action_exported, CompleteAction::Export),
                (self.action_file, CompleteAction::File),
                (self.action_group, CompleteAction::Group),
                (self.action_job, CompleteAction::Job),
                (self.action_keyword, CompleteAction::Keyword),
                (self.action_service, CompleteAction::Service),
                (self.action_user, CompleteAction::User),
                (self.action_variable, CompleteAction::Variable),
            ]
            .into_iter()
            .filter_map(|(enabled, action)| enabled.then_some(action)),
        );

        CompleteAction::value_variants()
            .iter()
            .filter(|action| actions.contains(action))
            .cloned()
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
        let names: Vec<&str> = match special {
            Some(special) => vec![special.command_name()],
            None => self.names.iter().map(String::as_str).collect(),
        };

        // With no spec named, list them all (as `complete` with no options does too), or
        // with `-r`, remove them all.
        if names.is_empty() {
            if self.remove && !self.print {
                context.shell.completion_config_mut().clear();
            } else {
                let config = context.shell.completion_config();
                // Sort, so the listing is stable; the special specs come last.
                let mut specs: Vec<_> = config
                    .iter()
                    .map(|(name, spec)| (name.as_str(), spec))
                    .collect();
                specs.sort_by_key(|(name, _)| *name);
                specs.extend(SpecialSpec::iter().filter_map(|special| {
                    Some((special.command_name(), config.get_special(special)?))
                }));
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
    /// Displays `spec`, registered under `name`: a command's, or bash's name for a special
    /// spec, which is shown as the flag that selects it.
    fn display_named_spec(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: &str,
        spec: &Spec,
    ) -> Result<(), brush_core::Error> {
        match SpecialSpec::from_command_name(name) {
            Some(special) => {
                Self::display_spec(context, Some(special_spec_flag(special)), None, spec)
            }
            None => Self::display_spec(context, None, Some(name), spec),
        }
    }

    fn try_display_spec_for_command(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: &str,
    ) -> Result<bool, brush_core::Error> {
        if let Some(spec) = context.shell.completion_config().get(name) {
            Self::display_named_spec(context, name, spec)?;
            Ok(true)
        } else {
            writeln!(
                context.stderr(),
                "complete: {name}: no completion specification"
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
        for (enabled, option) in [
            (options.bash_default, "bashdefault"),
            (options.default, "default"),
            (options.dir_names, "dirnames"),
            (options.file_names, "filenames"),
            (options.no_quote, "noquote"),
            (options.no_sort, "nosort"),
            (options.no_space, "nospace"),
            (options.plus_dirs, "plusdirs"),
        ] {
            if enabled {
                write!(s, " -o {option}")?;
            }
        }

        // Like bash, show each action once: those with their own flag first, then the rest
        // with `-A`, each in the order bash lists them.
        let actions: Vec<_> = spec.actions.iter().map(action_flag).collect();
        for own_flag in [true, false] {
            for flag in CompleteAction::value_variants().iter().map(action_flag) {
                if actions.contains(&flag) && flag.starts_with("-A ") != own_flag {
                    write!(s, " {flag}")?;
                }
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
        name: &str,
    ) -> Result<bool, brush_core::Error> {
        if self.print {
            return Self::try_display_spec_for_command(context, name);
        } else if self.remove {
            let mut result = context.shell.completion_config_mut().remove(name);

            if !result {
                if context.shell.options().interactive {
                    writeln!(context.stderr(), "complete: {name}: not found")?;
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

/// Returns the flag that selects `action` in `complete` and `compgen` (e.g. `-a`, or
/// `-A arrayvar` for an action with no flag of its own).
const fn action_flag(action: &CompleteAction) -> &'static str {
    match action {
        CompleteAction::Alias => "-a",
        CompleteAction::ArrayVar => "-A arrayvar",
        CompleteAction::Binding => "-A binding",
        CompleteAction::Builtin => "-b",
        CompleteAction::Command => "-c",
        CompleteAction::Directory => "-d",
        CompleteAction::Disabled => "-A disabled",
        CompleteAction::Enabled => "-A enabled",
        CompleteAction::Export => "-e",
        CompleteAction::File => "-f",
        CompleteAction::Function => "-A function",
        CompleteAction::Group => "-g",
        CompleteAction::HelpTopic => "-A helptopic",
        CompleteAction::HostName => "-A hostname",
        CompleteAction::Job => "-j",
        CompleteAction::Keyword => "-k",
        CompleteAction::Running => "-A running",
        CompleteAction::Service => "-s",
        CompleteAction::SetOpt => "-A setopt",
        CompleteAction::ShOpt => "-A shopt",
        CompleteAction::Signal => "-A signal",
        CompleteAction::Stopped => "-A stopped",
        CompleteAction::User => "-u",
        CompleteAction::Variable => "-v",
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
        spec.options.no_sort = true;

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
    #[arg(short = 'o', value_name = "OPT")]
    enabled_options: Vec<CompleteOption>,
    #[arg(long = concat!("+o"), hide = true)]
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
            options.insert(option.clone(), true);
        }
        for option in &self.disabled_options {
            options.insert(option.clone(), false);
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

        // -D, -E, and -I select a special spec, which, like bash, we name by bash's name for
        // it (e.g. in messages).
        let special = [
            (self.update_default, SpecialSpec::Default),
            (self.update_empty, SpecialSpec::EmptyLine),
            (self.update_initial_word, SpecialSpec::InitialWord),
        ]
        .into_iter()
        .find_map(|(selected, special)| selected.then_some(special));
        let names: Vec<&str> = match special {
            Some(special) => vec![special.command_name()],
            None => self.names.iter().map(String::as_str).collect(),
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
                        "compopt: {name}: no completion specification"
                    )?;
                    result = ExecutionResult::general_error();
                }
            }
            return Ok(result);
        }

        // With no names, apply to any completion actively in-flight.
        if let Some(in_flight_options) = context
            .shell
            .completion_config_mut()
            .current_completion_options
            .as_mut()
        {
            Self::set_options(in_flight_options, &options);
        }

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
            match option {
                CompleteOption::BashDefault => target_options.bash_default = *value,
                CompleteOption::Default => target_options.default = *value,
                CompleteOption::DirNames => target_options.dir_names = *value,
                CompleteOption::FileNames => target_options.file_names = *value,
                CompleteOption::NoQuote => target_options.no_quote = *value,
                CompleteOption::NoSort => target_options.no_sort = *value,
                CompleteOption::NoSpace => target_options.no_space = *value,
                CompleteOption::PlusDirs => target_options.plus_dirs = *value,
            }
        }
    }
}
