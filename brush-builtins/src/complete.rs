use bpaf::Parser;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::io::Write;

use brush_core::completion::{self, CompleteAction, CompleteOption, Spec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, escape};

struct CommonCompleteCommandArgs {
    options: Vec<CompleteOption>,
    actions: Vec<CompleteAction>,
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

impl CommonCompleteCommandArgs {
    fn parser() -> impl bpaf::Parser<Self> {
        let options = bpaf::short('o')
            .help("Options governing the behavior of completions.")
            .argument::<CompleteOption>("OPT")
            .many();
        let actions = bpaf::short('A')
            .help("Actions to apply to generate completions.")
            .argument::<CompleteAction>("ACTION")
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

/// Runs the given command's parser against the provided arguments.
///
// N.B. This mirrors `brush_core::builtins::run_parser`, which is not public.
fn run_parser<T: builtins::Command>(args: &[String]) -> Result<T, builtins::BuiltinArgParseError> {
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    T::parser()
        .to_options()
        .run_inner(os_args.as_slice())
        .map_err(render_parse_failure)
}

fn render_parse_failure(failure: bpaf::ParseFailure) -> builtins::BuiltinArgParseError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => builtins::BuiltinArgParseError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => builtins::BuiltinArgParseError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => builtins::BuiltinArgParseError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

/// Configure programmable command completion.
pub(crate) struct CompleteCommand {
    print: bool,
    remove: bool,
    use_as_default: bool,
    use_for_empty_line: bool,
    use_for_initial_word: bool,
    common_args: CommonCompleteCommandArgs,
    names: Vec<String>,
}

impl builtins::Command for CompleteCommand {
    type Error = brush_core::Error;

    /// Overrides the default [`builtins::Command::new`] flow to pre-join
    /// flag-looking values onto their value-taking options; see
    /// [`join_flag_looking_values`].
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        run_parser(&join_flag_looking_values(args))
    }

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

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();

        // N.B. A leading `--` operand ends the builtin's option section and is
        // not part of the command names.
        let names: &[String] = {
            let leading_markers = self
                .names
                .iter()
                .take_while(|name| name.as_str() == "--")
                .count();
            &self.names[leading_markers..]
        };

        // If -D, -E, or -I are specified, then any names provided are ignored.
        if self.use_as_default
            || self.use_for_empty_line
            || self.use_for_initial_word
            || names.is_empty()
        {
            self.process_global(&mut context)?;
        } else {
            for name in names {
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

/// Generate command completions.
pub(crate) struct CompGenCommand {
    common_args: CommonCompleteCommandArgs,

    // N.B. The word can only start with a hyphen if it's after a --.
    word: Option<String>,
}

impl builtins::Command for CompGenCommand {
    type Error = brush_core::Error;

    /// Overrides the default [`builtins::Command::new`] flow to pre-join
    /// flag-looking values onto their value-taking options; see
    /// [`join_flag_looking_values`].
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        run_parser(&join_flag_looking_values(args))
    }

    fn parser() -> impl bpaf::Parser<Self> {
        let common_args = CommonCompleteCommandArgs::parser();
        let word = bpaf::positional::<String>("WORD").optional();

        bpaf::construct!(CompGenCommand { common_args, word })
    }

    fn about() -> &'static str {
        "Generate command completions."
    }

    fn synopsis() -> &'static str {
        "[-o OPT]... [-A ACTION]... [WORD]"
    }

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
pub(crate) struct CompOptCommand {
    update_default: bool,
    update_empty: bool,
    update_initial_word: bool,
    enabled_options: Vec<CompleteOption>,
    disabled_options: Vec<CompleteOption>,
    names: Vec<String>,
}

impl builtins::Command for CompOptCommand {
    type Error = brush_core::Error;

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
            .argument::<CompleteOption>("OPT")
            .many();

        // N.B. The value may be adjacent to the tag (`+o OPT`); it cannot be
        // expressed as a simple argument parser because of the '+' spelling.
        let disabled_options = {
            let tag = bpaf::literal("+o");
            let val = bpaf::any("OPT", |opt: CompleteOption| Some(opt)).optional();
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
        "Set programmable command completion options."
    }

    fn synopsis() -> &'static str {
        "[-DEI] [-o OPT]... [+o OPT]... [NAME]..."
    }

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
