use clap::Parser;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;

use brush_core::completion::{self, CompleteAction, CompleteOption, Spec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, escape};
use strum::IntoEnumIterator;

/// Returns the name `-A` takes for `action`. The single source of these names:
/// parsing, `help`, error messages, and `complete -p` all derive from it.
const fn action_name(action: &CompleteAction) -> &'static str {
    match action {
        CompleteAction::Alias => "alias",
        CompleteAction::ArrayVar => "arrayvar",
        CompleteAction::Binding => "binding",
        CompleteAction::Builtin => "builtin",
        CompleteAction::Command => "command",
        CompleteAction::Directory => "directory",
        CompleteAction::Disabled => "disabled",
        CompleteAction::Enabled => "enabled",
        CompleteAction::Export => "export",
        CompleteAction::File => "file",
        CompleteAction::Function => "function",
        CompleteAction::Group => "group",
        CompleteAction::HelpTopic => "helptopic",
        CompleteAction::HostName => "hostname",
        CompleteAction::Job => "job",
        CompleteAction::Keyword => "keyword",
        CompleteAction::Running => "running",
        CompleteAction::Service => "service",
        CompleteAction::SetOpt => "setopt",
        CompleteAction::ShOpt => "shopt",
        CompleteAction::Signal => "signal",
        CompleteAction::Stopped => "stopped",
        CompleteAction::User => "user",
        CompleteAction::Variable => "variable",
    }
}

/// Returns the name `-o` takes for `option`. The single source of these names,
/// as with [`action_name`].
const fn option_name(option: &CompleteOption) -> &'static str {
    match option {
        CompleteOption::BashDefault => "bashdefault",
        CompleteOption::Default => "default",
        CompleteOption::DirNames => "dirnames",
        CompleteOption::FileNames => "filenames",
        CompleteOption::NoQuote => "noquote",
        CompleteOption::NoSort => "nosort",
        CompleteOption::NoSpace => "nospace",
        CompleteOption::PlusDirs => "plusdirs",
    }
}

/// Returns a clap value parser accepting the names `name_of` gives each variant
/// of `T`. clap lists those names in help and in errors.
fn named_variant_parser<T>(
    name_of: fn(&T) -> &'static str,
) -> impl clap::builder::TypedValueParser<Value = T>
where
    T: IntoEnumIterator + Clone + Send + Sync + 'static,
{
    use clap::builder::TypedValueParser as _;

    clap::builder::PossibleValuesParser::new(T::iter().map(|variant| name_of(&variant))).try_map(
        move |name| {
            T::iter()
                .find(|variant| name_of(variant) == name)
                .ok_or("unrecognized name")
        },
    )
}

/// Returns whether `option` is enabled in `options`.
const fn option_enabled(options: &completion::GenerationOptions, option: &CompleteOption) -> bool {
    match option {
        CompleteOption::BashDefault => options.bash_default,
        CompleteOption::Default => options.default,
        CompleteOption::DirNames => options.dir_names,
        CompleteOption::FileNames => options.file_names,
        CompleteOption::NoQuote => options.no_quote,
        CompleteOption::NoSort => options.no_sort,
        CompleteOption::NoSpace => options.no_space,
        CompleteOption::PlusDirs => options.plus_dirs,
    }
}

#[derive(Parser)]
struct CommonCompleteCommandArgs {
    /// Options governing the behavior of completions.
    #[arg(short = 'o', value_parser = named_variant_parser(option_name))]
    options: Vec<CompleteOption>,

    /// Actions to apply to generate completions.
    #[arg(short = 'A', value_parser = named_variant_parser(action_name))]
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

        actions
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

brush_builtin_utils::clap_builtin!(CompleteCommand);

impl builtins::Command for CompleteCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();

        // If -D, -E, or -I are specified, then any names provided are ignored.
        if self.use_as_default
            || self.use_for_empty_line
            || self.use_for_initial_word
            || self.names.is_empty()
        {
            self.process_global(&mut context)?;
        } else {
            for name in &self.names {
                if !self.try_process_for_command(&mut context, name.as_str())? {
                    result = ExecutionResult::general_error();
                }
            }
        }

        Ok(result)
    }
}

impl CompleteCommand {
    fn process_global(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<(), brush_core::Error> {
        // Read options before taking mutable borrow on completion_config
        let extended_globbing = context.shell.options().extended_globbing;

        // These are processed in an intentional order.
        let special_option_name;
        let target_spec = if self.use_as_default {
            special_option_name = "-D";
            Some(&mut context.shell.completion_config_mut().default)
        } else if self.use_for_empty_line {
            special_option_name = "-E";
            Some(&mut context.shell.completion_config_mut().empty_line)
        } else if self.use_for_initial_word {
            special_option_name = "-I";
            Some(&mut context.shell.completion_config_mut().initial_word)
        } else {
            special_option_name = "";
            None
        };

        // Treat 'complete' with no options the same as 'complete -p'.
        if self.print || (!self.remove && target_spec.is_none()) {
            if let Some(target_spec) = target_spec {
                if let Some(existing_spec) = target_spec {
                    let existing_spec = existing_spec.clone();
                    Self::display_spec(context, Some(special_option_name), None, &existing_spec)?;
                } else {
                    return error::unimp("special spec not found");
                }
            } else {
                for (command_name, spec) in context.shell.completion_config().iter() {
                    Self::display_spec(context, None, Some(command_name.as_str()), spec)?;
                }
            }
        } else if self.remove {
            if let Some(target_spec) = target_spec {
                let mut new_spec = None;
                std::mem::swap(&mut new_spec, target_spec);
            } else {
                context.shell.completion_config_mut().clear();
            }
        } else {
            if let Some(target_spec) = target_spec {
                let mut new_spec = Some(self.common_args.create_spec(extended_globbing));
                std::mem::swap(&mut new_spec, target_spec);
            } else {
                return error::unimp("set unspecified spec");
            }
        }

        Ok(())
    }

    fn try_display_spec_for_command(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: &str,
    ) -> Result<bool, brush_core::Error> {
        if let Some(spec) = context.shell.completion_config().get(name) {
            Self::display_spec(context, None, Some(name), spec)?;
            Ok(true)
        } else {
            writeln!(context.stderr(), "no completion found for command")?;
            Ok(false)
        }
    }

    fn display_spec(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        special_name: Option<&str>,
        command_name: Option<&str>,
        spec: &Spec,
    ) -> Result<(), brush_core::Error> {
        let mut s = String::from("complete");

        if let Some(special_name) = special_name {
            s.push(' ');
            s.push_str(special_name);
        }

        for action in &spec.actions {
            s.push(' ');

            let short_flag = match action {
                CompleteAction::Alias => Some("-a"),
                CompleteAction::Builtin => Some("-b"),
                CompleteAction::Command => Some("-c"),
                CompleteAction::Directory => Some("-d"),
                CompleteAction::Export => Some("-e"),
                CompleteAction::File => Some("-f"),
                CompleteAction::Group => Some("-g"),
                CompleteAction::Job => Some("-j"),
                CompleteAction::Keyword => Some("-k"),
                CompleteAction::Service => Some("-s"),
                CompleteAction::User => Some("-u"),
                CompleteAction::Variable => Some("-v"),
                _ => None,
            };

            match short_flag {
                Some(flag) => s.push_str(flag),
                None => write!(s, "-A {}", action_name(action))?,
            }
        }

        for option in CompleteOption::iter() {
            if option_enabled(&spec.options, &option) {
                write!(s, " -o {}", option_name(&option))?;
            }
        }

        if let Some(glob_pattern) = &spec.glob_pattern {
            write!(
                s,
                " -G {}",
                escape::force_quote(glob_pattern, escape::QuoteMode::SingleQuote)
            )?;
        }
        if let Some(word_list) = &spec.word_list {
            write!(
                s,
                " -W {}",
                escape::force_quote(word_list, escape::QuoteMode::SingleQuote)
            )?;
        }
        if let Some(function_name) = &spec.function_name {
            write!(s, " -F {function_name}")?;
        }
        if let Some(command) = &spec.command {
            write!(
                s,
                " -C {}",
                escape::force_quote(command, escape::QuoteMode::SingleQuote)
            )?;
        }
        if let Some(filter_pattern) = &spec.filter_pattern {
            write!(
                s,
                " -X {}",
                escape::force_quote(filter_pattern, escape::QuoteMode::SingleQuote)
            )?;
        }
        if let Some(prefix) = &spec.prefix {
            write!(
                s,
                " -P {}",
                escape::force_quote(prefix, escape::QuoteMode::SingleQuote)
            )?;
        }
        if let Some(suffix) = &spec.suffix {
            write!(
                s,
                " -S {}",
                escape::force_quote(suffix, escape::QuoteMode::SingleQuote)
            )?;
        }

        if let Some(command_name) = command_name {
            s.push(' ');
            s.push_str(command_name);
        }

        writeln!(context.stdout(), "{s}")?;

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

        let config = self
            .common_args
            .create_spec(context.shell.options().extended_globbing);

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

brush_builtin_utils::clap_builtin!(CompGenCommand);

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
    #[arg(short = 'o', value_name = "OPT", value_parser = named_variant_parser(option_name))]
    enabled_options: Vec<CompleteOption>,
    #[arg(long = concat!("+o"), hide = true, value_parser = named_variant_parser(option_name))]
    disabled_options: Vec<CompleteOption>,

    /// If specified, scopes updates to completions of the named commands.
    names: Vec<String>,
}

brush_builtin_utils::clap_builtin!(CompOptCommand);

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

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;
    use clap::CommandFactory;

    #[test]
    fn every_action_name_parses_to_its_action() -> Result<()> {
        for action in CompleteAction::iter() {
            let args = CommonCompleteCommandArgs::try_parse_from([
                "complete",
                "-A",
                action_name(&action),
            ])?;
            let parsed: Vec<_> = args.actions.iter().map(action_name).collect();
            assert_eq!(parsed, [action_name(&action)]);
        }
        Ok(())
    }

    #[test]
    fn every_option_name_parses_to_its_option() -> Result<()> {
        for option in CompleteOption::iter() {
            let name = option_name(&option);
            let args = CommonCompleteCommandArgs::try_parse_from(["complete", "-o", name])?;
            assert_eq!(args.options, std::slice::from_ref(&option));

            let args = CompOptCommand::try_parse_from(["compopt", "--+o", name])?;
            assert_eq!(args.disabled_options, [option]);
        }
        Ok(())
    }

    #[test]
    fn invalid_names_list_the_accepted_ones() {
        let Err(err) = CommonCompleteCommandArgs::try_parse_from(["complete", "-A", "bogus"])
        else {
            unreachable!("`bogus` is not an action name");
        };
        let message = err.to_string();
        assert!(
            message.contains("possible values") && message.contains("arrayvar"),
            "{message}"
        );
    }

    #[test]
    fn help_lists_accepted_names() {
        let help = CompleteCommand::command().render_help().to_string();
        assert!(
            help.contains("arrayvar") && help.contains("nospace"),
            "{help}"
        );
    }
}
