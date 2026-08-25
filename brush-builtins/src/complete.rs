use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use std::str::FromStr;

use brush_core::argmodel::{ArgSpec, CommandSpec, ParsedValues, PositionalSpec};
use brush_core::completion::{self, CompleteAction, CompleteOption, Spec};
use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, escape};

const ID_OPTIONS: &str = "options";
const ID_ACTIONS: &str = "actions";
const ID_GLOB_PATTERN: &str = "glob_pattern";
const ID_WORD_LIST: &str = "word_list";
const ID_FUNCTION_NAME: &str = "function_name";
const ID_COMMAND: &str = "command";
const ID_FILTER_PATTERN: &str = "filter_pattern";
const ID_PREFIX: &str = "prefix";
const ID_SUFFIX: &str = "suffix";
const ID_ACTION_ALIAS: &str = "action_alias";
const ID_ACTION_BUILTIN: &str = "action_builtin";
const ID_ACTION_COMMAND: &str = "action_command";
const ID_ACTION_DIRECTORY: &str = "action_directory";
const ID_ACTION_EXPORTED: &str = "action_exported";
const ID_ACTION_FILE: &str = "action_file";
const ID_ACTION_GROUP: &str = "action_group";
const ID_ACTION_JOB: &str = "action_job";
const ID_ACTION_KEYWORD: &str = "action_keyword";
const ID_ACTION_SERVICE: &str = "action_service";
const ID_ACTION_USER: &str = "action_user";
const ID_ACTION_VARIABLE: &str = "action_variable";

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
    fn from_matches(values: &ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        let mut options = Vec::new();
        for value in values.values(ID_OPTIONS) {
            options.push(value.parse().map_err(|_| invalid_value("-o", value))?);
        }

        let mut actions = Vec::new();
        for value in values.values(ID_ACTIONS) {
            actions.push(value.parse().map_err(|_| invalid_value("-A", value))?);
        }

        Ok(Self {
            options,
            actions,
            glob_pattern: values.value(ID_GLOB_PATTERN).map(str::to_owned),
            word_list: values.value(ID_WORD_LIST).map(str::to_owned),
            function_name: values.value(ID_FUNCTION_NAME).map(str::to_owned),
            command: values.value(ID_COMMAND).map(str::to_owned),
            filter_pattern: values.value(ID_FILTER_PATTERN).map(str::to_owned),
            prefix: values.value(ID_PREFIX).map(str::to_owned),
            suffix: values.value(ID_SUFFIX).map(str::to_owned),
            action_alias: values.flag(ID_ACTION_ALIAS),
            action_builtin: values.flag(ID_ACTION_BUILTIN),
            action_command: values.flag(ID_ACTION_COMMAND),
            action_directory: values.flag(ID_ACTION_DIRECTORY),
            action_exported: values.flag(ID_ACTION_EXPORTED),
            action_file: values.flag(ID_ACTION_FILE),
            action_group: values.flag(ID_ACTION_GROUP),
            action_job: values.flag(ID_ACTION_JOB),
            action_keyword: values.flag(ID_ACTION_KEYWORD),
            action_service: values.flag(ID_ACTION_SERVICE),
            action_user: values.flag(ID_ACTION_USER),
            action_variable: values.flag(ID_ACTION_VARIABLE),
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

/// Returns a rendered parse error for an invalid option/action value.
fn invalid_value(option: &str, value: &str) -> builtins::BuiltinArgParseError {
    builtins::BuiltinArgParseError {
        message: format!("invalid value for {option}: `{value}`"),
        help_request: false,
    }
}

/// Parses the given command against the provided arguments, pre-joining
/// flag-looking values onto their value-taking options; see
/// [`join_flag_looking_values`].
fn parse_joined<T: builtins::SpecCommand>(
    args: Vec<String>,
) -> Result<T, builtins::BuiltinArgParseError> {
    let joined = join_flag_looking_values(args);

    let mut values = brush_core::builtins::argmodel::backend().parse(T::spec(), "", &joined)?;

    T::from_matches(&mut values)
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

static COMPLETE_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::value(
            ID_OPTIONS,
            &['o'],
            &[],
            "OPT",
            "Options governing the behavior of completions.",
        ),
        ArgSpec::value(
            ID_ACTIONS,
            &['A'],
            &[],
            "ACTION",
            "Actions to apply to generate completions.",
        ),
        ArgSpec::value(
            ID_GLOB_PATTERN,
            &['G'],
            &[],
            "GLOB",
            "File glob pattern to be expanded to generate completions.",
        ),
        ArgSpec::value(
            ID_WORD_LIST,
            &['W'],
            &[],
            "WORD_LIST",
            "List of words that will be considered as completions.",
        ),
        ArgSpec::value(
            ID_FUNCTION_NAME,
            &['F'],
            &[],
            "FUNC_NAME",
            "Name of a shell function to invoke to generate completions.",
        ),
        ArgSpec::value(
            ID_COMMAND,
            &['C'],
            &[],
            "COMMAND",
            "Command to execute to generate completions.",
        ),
        ArgSpec::value(
            ID_FILTER_PATTERN,
            &['X'],
            &[],
            "PATTERN",
            "Pattern used as filter for completions.",
        ),
        ArgSpec::value(
            ID_PREFIX,
            &['P'],
            &[],
            "PREFIX",
            "Prefix pattern used as filter for completions.",
        ),
        ArgSpec::value(
            ID_SUFFIX,
            &['S'],
            &[],
            "SUFFIX",
            "Suffix pattern used as filter for completions.",
        ),
        ArgSpec::flag(ID_ACTION_ALIAS, &['a'], &[], "Complete with valid aliases."),
        ArgSpec::flag(
            ID_ACTION_BUILTIN,
            &['b'],
            &[],
            "Complete with names of shell builtins.",
        ),
        ArgSpec::flag(
            ID_ACTION_COMMAND,
            &['c'],
            &[],
            "Complete with names of executable commands.",
        ),
        ArgSpec::flag(
            ID_ACTION_DIRECTORY,
            &['d'],
            &[],
            "Complete with directory names.",
        ),
        ArgSpec::flag(
            ID_ACTION_EXPORTED,
            &['e'],
            &[],
            "Complete with names of exported shell variables.",
        ),
        ArgSpec::flag(ID_ACTION_FILE, &['f'], &[], "Complete with filenames."),
        ArgSpec::flag(
            ID_ACTION_GROUP,
            &['g'],
            &[],
            "Complete with valid user groups.",
        ),
        ArgSpec::flag(ID_ACTION_JOB, &['j'], &[], "Complete with job specs."),
        ArgSpec::flag(ID_ACTION_KEYWORD, &['k'], &[], "Complete with keywords."),
        ArgSpec::flag(
            ID_ACTION_SERVICE,
            &['s'],
            &[],
            "Complete with names of system services.",
        ),
        ArgSpec::flag(
            ID_ACTION_USER,
            &['u'],
            &[],
            "Complete with valid usernames.",
        ),
        ArgSpec::flag(
            ID_ACTION_VARIABLE,
            &['v'],
            &[],
            "Complete with names of shell variables.",
        ),
        ArgSpec::flag(
            "print",
            &['p'],
            &[],
            "Display registered completion settings.",
        ),
        ArgSpec::flag(
            "remove",
            &['r'],
            &[],
            "Remove the completion settings associated with the given command.",
        ),
        ArgSpec::flag(
            "use_as_default",
            &['D'],
            &[],
            "Apply these settings to the default completion scenario.",
        ),
        ArgSpec::flag(
            "use_for_empty_line",
            &['E'],
            &[],
            "Apply these settings to completion of empty lines.",
        ),
        ArgSpec::flag(
            "use_for_initial_word",
            &['I'],
            &[],
            "Apply these settings to completion of the initial word of the input line.",
        ),
    ],
    positionals: &[PositionalSpec::many("names", "NAMES")],
};

impl builtins::SpecCommand for CompleteCommand {
    type Error = brush_core::Error;

    /// Overrides the default [`builtins::SpecCommand::new`] flow to pre-join
    /// flag-looking values onto their value-taking options; see
    /// [`join_flag_looking_values`].
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        parse_joined(args)
    }

    fn spec() -> &'static CommandSpec {
        &COMPLETE_SPEC
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            print: values.flag("print"),
            remove: values.flag("remove"),
            use_as_default: values.flag("use_as_default"),
            use_for_empty_line: values.flag("use_for_empty_line"),
            use_for_initial_word: values.flag("use_for_initial_word"),
            common_args: CommonCompleteCommandArgs::from_matches(values)?,
            names: values.positional_values("names").to_vec(),
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

static COMPGEN_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::value(
            ID_OPTIONS,
            &['o'],
            &[],
            "OPT",
            "Options governing the behavior of completions.",
        ),
        ArgSpec::value(
            ID_ACTIONS,
            &['A'],
            &[],
            "ACTION",
            "Actions to apply to generate completions.",
        ),
        ArgSpec::value(
            ID_GLOB_PATTERN,
            &['G'],
            &[],
            "GLOB",
            "File glob pattern to be expanded to generate completions.",
        ),
        ArgSpec::value(
            ID_WORD_LIST,
            &['W'],
            &[],
            "WORD_LIST",
            "List of words that will be considered as completions.",
        ),
        ArgSpec::value(
            ID_FUNCTION_NAME,
            &['F'],
            &[],
            "FUNC_NAME",
            "Name of a shell function to invoke to generate completions.",
        ),
        ArgSpec::value(
            ID_COMMAND,
            &['C'],
            &[],
            "COMMAND",
            "Command to execute to generate completions.",
        ),
        ArgSpec::value(
            ID_FILTER_PATTERN,
            &['X'],
            &[],
            "PATTERN",
            "Pattern used as filter for completions.",
        ),
        ArgSpec::value(
            ID_PREFIX,
            &['P'],
            &[],
            "PREFIX",
            "Prefix pattern used as filter for completions.",
        ),
        ArgSpec::value(
            ID_SUFFIX,
            &['S'],
            &[],
            "SUFFIX",
            "Suffix pattern used as filter for completions.",
        ),
        ArgSpec::flag(ID_ACTION_ALIAS, &['a'], &[], "Complete with valid aliases."),
        ArgSpec::flag(
            ID_ACTION_BUILTIN,
            &['b'],
            &[],
            "Complete with names of shell builtins.",
        ),
        ArgSpec::flag(
            ID_ACTION_COMMAND,
            &['c'],
            &[],
            "Complete with names of executable commands.",
        ),
        ArgSpec::flag(
            ID_ACTION_DIRECTORY,
            &['d'],
            &[],
            "Complete with directory names.",
        ),
        ArgSpec::flag(
            ID_ACTION_EXPORTED,
            &['e'],
            &[],
            "Complete with names of exported shell variables.",
        ),
        ArgSpec::flag(ID_ACTION_FILE, &['f'], &[], "Complete with filenames."),
        ArgSpec::flag(
            ID_ACTION_GROUP,
            &['g'],
            &[],
            "Complete with valid user groups.",
        ),
        ArgSpec::flag(ID_ACTION_JOB, &['j'], &[], "Complete with job specs."),
        ArgSpec::flag(ID_ACTION_KEYWORD, &['k'], &[], "Complete with keywords."),
        ArgSpec::flag(
            ID_ACTION_SERVICE,
            &['s'],
            &[],
            "Complete with names of system services.",
        ),
        ArgSpec::flag(
            ID_ACTION_USER,
            &['u'],
            &[],
            "Complete with valid usernames.",
        ),
        ArgSpec::flag(
            ID_ACTION_VARIABLE,
            &['v'],
            &[],
            "Complete with names of shell variables.",
        ),
    ],
    positionals: &[PositionalSpec::one("word", "WORD")],
};

impl builtins::SpecCommand for CompGenCommand {
    type Error = brush_core::Error;

    /// Overrides the default [`builtins::SpecCommand::new`] flow to pre-join
    /// flag-looking values onto their value-taking options; see
    /// [`join_flag_looking_values`].
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        parse_joined(args)
    }

    fn spec() -> &'static CommandSpec {
        &COMPGEN_SPEC
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            common_args: CommonCompleteCommandArgs::from_matches(values)?,

            // N.B. The word can only start with a hyphen if it's after a --.
            word: values.value_of_positional("word").map(str::to_owned),
        })
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

static COMPOPT_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag(
            "update_default",
            &['D'],
            &[],
            "Update the default completion settings.",
        ),
        ArgSpec::flag(
            "update_empty",
            &['E'],
            &[],
            "Update the completion settings for empty lines.",
        ),
        ArgSpec::flag(
            "update_initial_word",
            &['I'],
            &[],
            "Update the completion settings for the initial word of the input line.",
        ),
        ArgSpec::value(
            ID_OPTIONS,
            &['o'],
            &[],
            "OPT",
            "Enable the specified option for selected completion scenarios.",
        ),
        // N.B. Declared for help rendering; `+o` occurrences are extracted
        // from the token stream before the backend parses.
        ArgSpec::hidden_value("disabled_options", &[], &["+o"], "OPT", ""),
    ],
    positionals: &[PositionalSpec::many("names", "NAMES")],
};

impl builtins::SpecCommand for CompOptCommand {
    type Error = brush_core::Error;

    /// Overrides the default [`builtins::SpecCommand::new`] flow to pre-join
    /// flag-looking values onto their value-taking options (see
    /// [`join_flag_looking_values`]) and to extract `+o` occurrences, whose
    /// optional values the backend's required-value options cannot express.
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        // N.B. The first argument is the command name itself.
        let args: Vec<String> = args.into_iter().skip(1).collect();
        let joined = join_flag_looking_values(args);

        let mut disabled_options = Vec::new();
        let mut remaining = Vec::with_capacity(joined.len());
        let mut iter = joined.into_iter().peekable();

        while let Some(arg) = iter.next() {
            if arg == "+o" {
                // Consume a following word as the option value when it parses;
                // otherwise the occurrence enables no specific option.
                if let Some(next) = iter.peek()
                    && let Ok(option) = CompleteOption::from_str(next.as_str())
                {
                    iter.next();
                    disabled_options.push(option);
                }
            } else {
                remaining.push(arg);
            }
        }

        let mut values =
            brush_core::builtins::argmodel::backend().parse(Self::spec(), "", &remaining)?;

        let mut command = Self::from_matches(&mut values)?;
        command.disabled_options = disabled_options;

        Ok(command)
    }

    fn spec() -> &'static CommandSpec {
        &COMPOPT_SPEC
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        let mut enabled_options = Vec::new();
        for value in values.values(ID_OPTIONS) {
            enabled_options.push(value.parse().map_err(|_| invalid_value("-o", value))?);
        }

        Ok(Self {
            update_default: values.flag("update_default"),
            update_empty: values.flag("update_empty"),
            update_initial_word: values.flag("update_initial_word"),
            enabled_options,
            disabled_options: Vec::new(),
            names: values.positional_values("names").to_vec(),
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
