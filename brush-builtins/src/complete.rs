//! The `complete` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(CompleteCommand);

// N.B. Resolve to whichever engine was selected by `arg_impl!` (priority:
// parser-usage > parser-bpaf > parser-clap).
use imp::CommonCompleteCommandArgs;
pub(crate) use imp::{CompGenCommand, CompOptCommand};

use brush_core::completion::{self, CompleteAction, CompleteOption, Spec};
use brush_core::{ExecutionResult, error, escape};
use std::fmt::Write as _;
use std::io::Write;

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

    #[expect(clippy::too_many_lines)]
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

            let action_str = match action {
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
            };

            s.push_str(action_str);
        }

        if spec.options.bash_default {
            s.push_str(" -o bashdefault");
        }
        if spec.options.default {
            s.push_str(" -o default");
        }
        if spec.options.dir_names {
            s.push_str(" -o dirnames");
        }
        if spec.options.file_names {
            s.push_str(" -o filenames");
        }
        if spec.options.no_quote {
            s.push_str(" -o noquote");
        }
        if spec.options.no_sort {
            s.push_str(" -o nosort");
        }
        if spec.options.no_space {
            s.push_str(" -o nospace");
        }
        if spec.options.plus_dirs {
            s.push_str(" -o plusdirs");
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

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &CompleteCommand,
    mut context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut result = ExecutionResult::success();

    // If -D, -E, or -I are specified, then any names provided are ignored.
    if command.use_as_default
        || command.use_for_empty_line
        || command.use_for_initial_word
        || command.names.is_empty()
    {
        command.process_global(&mut context)?;
    } else {
        for name in &command.names {
            if !command.try_process_for_command(&mut context, name.as_str())? {
                result = ExecutionResult::general_error();
            }
        }
    }

    Ok(result)
}
