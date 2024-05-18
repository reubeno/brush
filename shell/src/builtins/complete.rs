use clap::{arg, Parser};
use std::collections::HashMap;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use crate::completion::{self, CompleteAction, CompleteOption, CompletionSpec};
use crate::error;

#[derive(Parser)]
pub(crate) struct CommonCompleteCommandArgs {
    #[arg(short = 'o')]
    options: Vec<CompleteOption>,

    #[arg(short = 'A')]
    actions: Vec<CompleteAction>,

    #[arg(short = 'G')]
    glob_pattern: Option<String>,

    #[arg(short = 'W')]
    word_list: Option<String>,

    #[arg(short = 'F')]
    function_name: Option<String>,

    #[arg(short = 'C')]
    command: Option<String>,

    #[arg(short = 'X')]
    filter_pattern: Option<String>,

    #[arg(short = 'P')]
    prefix: Option<String>,

    #[arg(short = 'S')]
    suffix: Option<String>,

    #[arg(short = 'a')]
    action_alias: bool,

    #[arg(short = 'b')]
    action_builtin: bool,

    #[arg(short = 'c')]
    action_command: bool,

    #[arg(short = 'd')]
    action_directory: bool,

    #[arg(short = 'e')]
    action_exported: bool,

    #[arg(short = 'f')]
    action_file: bool,

    #[arg(short = 'g')]
    action_group: bool,

    #[arg(short = 'j')]
    action_job: bool,

    #[arg(short = 'k')]
    action_keyword: bool,

    #[arg(short = 's')]
    action_service: bool,

    #[arg(short = 'u')]
    action_user: bool,

    #[arg(short = 'v')]
    action_variable: bool,
}

impl CommonCompleteCommandArgs {
    fn create_spec(&self) -> completion::CompletionSpec {
        let filter_pattern_excludes;
        let filter_pattern = if let Some(filter_pattern) = self.filter_pattern.as_ref() {
            if let Some(filter_pattern) = filter_pattern.strip_prefix('!') {
                filter_pattern_excludes = false;
                Some(filter_pattern.to_owned())
            } else {
                filter_pattern_excludes = true;
                Some(filter_pattern.clone())
            }
        } else {
            filter_pattern_excludes = false;
            None
        };

        let mut spec = completion::CompletionSpec {
            bash_default: false,
            default: false,
            dir_names: false,
            file_names: false,
            no_quote: false,
            no_sort: false,
            no_space: false,
            plus_dirs: false,
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
                CompleteOption::BashDefault => spec.bash_default = true,
                CompleteOption::Default => spec.default = true,
                CompleteOption::DirNames => spec.dir_names = true,
                CompleteOption::FileNames => spec.file_names = true,
                CompleteOption::NoQuote => spec.no_quote = true,
                CompleteOption::NoSort => spec.no_sort = true,
                CompleteOption::NoSpace => spec.no_space = true,
                CompleteOption::PlusDirs => spec.plus_dirs = true,
            }
        }

        spec
    }

    fn resolve_actions(&self) -> Vec<CompleteAction> {
        let mut actions = self.actions.clone();

        if self.action_alias {
            actions.push(CompleteAction::Alias);
        }
        if self.action_builtin {
            actions.push(CompleteAction::Builtin);
        }
        if self.action_command {
            actions.push(CompleteAction::Command);
        }
        if self.action_directory {
            actions.push(CompleteAction::Directory);
        }
        if self.action_exported {
            actions.push(CompleteAction::Export);
        }
        if self.action_file {
            actions.push(CompleteAction::File);
        }
        if self.action_group {
            actions.push(CompleteAction::Group);
        }
        if self.action_job {
            actions.push(CompleteAction::Job);
        }
        if self.action_keyword {
            actions.push(CompleteAction::Keyword);
        }
        if self.action_service {
            actions.push(CompleteAction::Service);
        }
        if self.action_user {
            actions.push(CompleteAction::User);
        }
        if self.action_variable {
            actions.push(CompleteAction::Variable);
        }

        actions
    }
}

/// Configure programmable command completion.
#[derive(Parser)]
pub(crate) struct CompleteCommand {
    #[arg(short = 'p')]
    print: bool,

    #[arg(short = 'r')]
    remove: bool,

    #[arg(short = 'D')]
    use_as_default: bool,

    #[arg(short = 'E')]
    use_for_empty_line: bool,

    #[arg(short = 'I')]
    use_for_initial_word: bool,

    #[clap(flatten)]
    common_args: CommonCompleteCommandArgs,

    names: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for CompleteCommand {
    async fn execute(
        &self,
        mut context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        // If -D, -E, or -I are specified, then any names provided are ignored.
        if self.use_as_default
            || self.use_for_empty_line
            || self.use_for_initial_word
            || self.names.is_empty()
        {
            self.process_global(&mut context)?;
        } else {
            for name in &self.names {
                self.process_for_command(&mut context, name.as_str())?;
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}

impl CompleteCommand {
    fn process_global(
        &self,
        context: &mut crate::context::CommandExecutionContext<'_>,
    ) -> Result<(), crate::error::Error> {
        // These are processed in an intentional order.
        let special_option_name;
        let target_spec = if self.use_as_default {
            special_option_name = "-D";
            Some(&mut context.shell.completion_config.default)
        } else if self.use_for_empty_line {
            special_option_name = "-E";
            Some(&mut context.shell.completion_config.empty_line)
        } else if self.use_for_initial_word {
            special_option_name = "-I";
            Some(&mut context.shell.completion_config.initial_word)
        } else {
            special_option_name = "";
            None
        };

        if self.print {
            if let Some(target_spec) = target_spec {
                if let Some(existing_spec) = target_spec {
                    let existing_spec = existing_spec.clone();
                    Self::display_spec(context, Some(special_option_name), None, &existing_spec)?;
                } else {
                    return error::unimp("special spec not found");
                }
            } else {
                for (command_name, spec) in &context.shell.completion_config.commands {
                    Self::display_spec(context, None, Some(command_name.as_str()), spec)?;
                }
            }
        } else if self.remove {
            if let Some(target_spec) = target_spec {
                let mut new_spec = None;
                std::mem::swap(&mut new_spec, target_spec);
            } else {
                return error::unimp("remove all specs");
            }
        } else {
            if let Some(target_spec) = target_spec {
                let mut new_spec = Some(self.common_args.create_spec());
                std::mem::swap(&mut new_spec, target_spec);
            } else {
                return error::unimp("set unspecified spec");
            }
        }

        Ok(())
    }

    fn display_spec_for_command(
        context: &mut crate::context::CommandExecutionContext<'_>,
        name: &str,
    ) -> Result<(), error::Error> {
        if let Some(spec) = context.shell.completion_config.commands.get(name) {
            Self::display_spec(context, None, Some(name), spec)
        } else {
            error::unimp("no completion found for command")
        }
    }

    fn display_spec(
        context: &crate::context::CommandExecutionContext<'_>,
        special_name: Option<&str>,
        command_name: Option<&str>,
        spec: &CompletionSpec,
    ) -> Result<(), error::Error> {
        let mut s = String::from("complete");

        if let Some(special_name) = special_name {
            s.push(' ');
            s.push_str(special_name);
        }

        for action in &spec.actions {
            let action_str = match action {
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
            };

            let piece = std::format!(" -A {action_str}");
            s.push_str(&piece);
        }

        if spec.bash_default {
            s.push_str(" -o bashdefault");
        }
        if spec.default {
            s.push_str(" -o default");
        }
        if spec.dir_names {
            s.push_str(" -o dirnames");
        }
        if spec.file_names {
            s.push_str(" -o filenames");
        }
        if spec.no_quote {
            s.push_str(" -o noquote");
        }
        if spec.no_sort {
            s.push_str(" -o nosort");
        }
        if spec.no_space {
            s.push_str(" -o nospace");
        }
        if spec.plus_dirs {
            s.push_str(" -o plusdirs");
        }

        if let Some(glob_pattern) = &spec.glob_pattern {
            s.push_str(&std::format!(" -G {glob_pattern}"));
        }
        if let Some(word_list) = &spec.word_list {
            s.push_str(&std::format!(" -W {word_list}"));
        }
        if let Some(function_name) = &spec.function_name {
            s.push_str(&std::format!(" -F {function_name}"));
        }
        if let Some(command) = &spec.command {
            s.push_str(&std::format!(" -C {command}"));
        }
        if let Some(filter_pattern) = &spec.filter_pattern {
            s.push_str(&std::format!(" -X {filter_pattern}"));
        }
        if let Some(prefix) = &spec.prefix {
            s.push_str(&std::format!(" -P {prefix}"));
        }
        if let Some(suffix) = &spec.suffix {
            s.push_str(&std::format!(" -S {suffix}"));
        }

        if let Some(command_name) = command_name {
            s.push(' ');
            s.push_str(command_name);
        }

        writeln!(context.stdout(), "{s}")?;

        Ok(())
    }

    fn process_for_command(
        &self,
        context: &mut crate::context::CommandExecutionContext<'_>,
        name: &str,
    ) -> Result<(), crate::error::Error> {
        if self.print {
            return Self::display_spec_for_command(context, name);
        } else if self.remove {
            context.shell.completion_config.commands.remove(name);
            return Ok(());
        }

        let config = self.common_args.create_spec();

        context
            .shell
            .completion_config
            .commands
            .insert(name.to_owned(), config);

        Ok(())
    }
}

/// Generate command completions.
#[derive(Parser)]
pub(crate) struct CompGenCommand {
    #[clap(flatten)]
    common_args: CommonCompleteCommandArgs,

    word: Option<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for CompGenCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let spec = self.common_args.create_spec();

        let token_to_complete = self.word.as_deref().unwrap_or_default();

        let completion_context = completion::CompletionContext {
            token_to_complete,
            preceding_token: None,
            command_name: None,
            token_index: 0,
            tokens: &[&parser::Token::Word(
                token_to_complete.to_owned(),
                parser::TokenLocation::default(),
            )],
            input_line: token_to_complete,
            cursor_index: token_to_complete.len(),
        };

        let result = spec
            .get_completions(context.shell, &completion_context)
            .await?;

        match result {
            completion::CompletionResult::Candidates(candidates) => {
                for candidate in candidates {
                    writeln!(context.stdout(), "{candidate}")?;
                }
            }
            completion::CompletionResult::RestartCompletionProcess => {
                return error::unimp("restart completion")
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}

/// Set programmable command completion options.
#[derive(Parser)]
pub(crate) struct CompOptCommand {
    #[arg(short = 'D')]
    update_default: bool,

    #[arg(short = 'E')]
    update_empty: bool,

    #[arg(short = 'I')]
    update_initial_word: bool,

    #[arg(short = 'o')]
    enabled_options: Vec<CompleteOption>,
    #[arg(long = concat!("+o"), hide = true)]
    disabled_options: Vec<CompleteOption>,

    names: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for CompOptCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if !self.names.is_empty() {
            tracing::debug!("UNIMPLEMENTED: compopt with names");
            return error::unimp("compopt with names");
        }

        let target_spec = if self.update_default {
            Some(&mut context.shell.completion_config.default)
        } else if self.update_empty {
            Some(&mut context.shell.completion_config.empty_line)
        } else if self.update_initial_word {
            Some(&mut context.shell.completion_config.initial_word)
        } else {
            None
        };

        let mut options = HashMap::new();
        for option in &self.disabled_options {
            options.insert(option.clone(), false);
        }
        for option in &self.enabled_options {
            options.insert(option.clone(), true);
        }

        if let Some(Some(target_spec)) = target_spec {
            for (option, value) in options {
                match option {
                    CompleteOption::BashDefault => target_spec.bash_default = value,
                    CompleteOption::Default => target_spec.default = value,
                    CompleteOption::DirNames => target_spec.dir_names = value,
                    CompleteOption::FileNames => target_spec.file_names = value,
                    CompleteOption::NoQuote => target_spec.no_quote = value,
                    CompleteOption::NoSort => target_spec.no_sort = value,
                    CompleteOption::NoSpace => target_spec.no_space = value,
                    CompleteOption::PlusDirs => target_spec.plus_dirs = value,
                }
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
