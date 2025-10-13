//! Implements programmable command completion support.

use clap::ValueEnum;
use indexmap::IndexSet;
use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    Shell, commands, env, error, escape, jobs, namedoptions, patterns,
    sys::{self, users},
    trace_categories, traps,
    variables::{self, ShellValueLiteral},
};

/// Type of action to take to generate completion candidates.
#[derive(Clone, Debug, ValueEnum)]
pub enum CompleteAction {
    /// Complete with valid aliases.
    #[clap(name = "alias")]
    Alias,
    /// Complete with names of array shell variables.
    #[clap(name = "arrayvar")]
    ArrayVar,
    /// Complete with names of key bindings.
    #[clap(name = "binding")]
    Binding,
    /// Complete with names of shell builtins.
    #[clap(name = "builtin")]
    Builtin,
    /// Complete with names of executable commands.
    #[clap(name = "command")]
    Command,
    /// Complete with directory names.
    #[clap(name = "directory")]
    Directory,
    /// Complete with names of disabled shell builtins.
    #[clap(name = "disabled")]
    Disabled,
    /// Complete with names of enabled shell builtins.
    #[clap(name = "enabled")]
    Enabled,
    /// Complete with names of exported shell variables.
    #[clap(name = "export")]
    Export,
    /// Complete with filenames.
    #[clap(name = "file")]
    File,
    /// Complete with names of shell functions.
    #[clap(name = "function")]
    Function,
    /// Complete with valid user groups.
    #[clap(name = "group")]
    Group,
    /// Complete with names of valid shell help topics.
    #[clap(name = "helptopic")]
    HelpTopic,
    /// Complete with the system's hostname(s).
    #[clap(name = "hostname")]
    HostName,
    /// Complete with the command names of shell-managed jobs.
    #[clap(name = "job")]
    Job,
    /// Complete with valid shell keywords.
    #[clap(name = "keyword")]
    Keyword,
    /// Complete with the command names of running shell-managed jobs.
    #[clap(name = "running")]
    Running,
    /// Complete with names of system services.
    #[clap(name = "service")]
    Service,
    /// Complete with the names of options settable via shopt.
    #[clap(name = "setopt")]
    SetOpt,
    /// Complete with the names of options settable via set -o.
    #[clap(name = "shopt")]
    ShOpt,
    /// Complete with the names of trappable signals.
    #[clap(name = "signal")]
    Signal,
    /// Complete with the command names of stopped shell-managed jobs.
    #[clap(name = "stopped")]
    Stopped,
    /// Complete with valid usernames.
    #[clap(name = "user")]
    User,
    /// Complete with names of shell variables.
    #[clap(name = "variable")]
    Variable,
}

/// Options influencing how command completions are generated.
#[derive(Clone, Debug, Eq, Hash, PartialEq, ValueEnum)]
pub enum CompleteOption {
    /// Perform rest of default completions if no completions are generated.
    #[clap(name = "bashdefault")]
    BashDefault,
    /// Use default filename completion if no completions are generated.
    #[clap(name = "default")]
    Default,
    /// Treat completions as directory names.
    #[clap(name = "dirnames")]
    DirNames,
    /// Treat completions as filenames.
    #[clap(name = "filenames")]
    FileNames,
    /// Suppress default auto-quotation of completions.
    #[clap(name = "noquote")]
    NoQuote,
    /// Do not sort completions.
    #[clap(name = "nosort")]
    NoSort,
    /// Do not append a trailing space to completions at the end of the input line.
    #[clap(name = "nospace")]
    NoSpace,
    /// Also generate directory completions.
    #[clap(name = "plusdirs")]
    PlusDirs,
}

/// Encapsulates the shell's programmable command completion configuration.
#[derive(Clone, Default)]
pub struct Config {
    commands: HashMap<String, Spec>,

    /// Optionally, a completion spec to be used as a default, when earlier
    /// matches yield no candidates.
    pub default: Option<Spec>,
    /// Optionally, a completion spec to be used when the command line is empty.
    pub empty_line: Option<Spec>,
    /// Optionally, a completion spec to be used for the initial word of a command line.
    pub initial_word: Option<Spec>,

    /// Optionally, stores the current completion options in effect. May be mutated
    /// while a completion generation is in-flight.
    pub current_completion_options: Option<GenerationOptions>,
}

/// Options for generating completions.
#[derive(Clone, Debug, Default)]
pub struct GenerationOptions {
    //
    // Options
    /// Perform rest of default completions if no completions are generated.
    pub bash_default: bool,
    /// Use default readline-style filename completion if no completions are generated.
    pub default: bool,
    /// Treat completions as directory names.
    pub dir_names: bool,
    /// Treat completions as filenames.
    pub file_names: bool,
    /// Do not add usual quoting for completions.
    pub no_quote: bool,
    /// Do not sort completions.
    pub no_sort: bool,
    /// Do not append typical space to a completion at the end of the input line.
    pub no_space: bool,
    /// Also complete with directory names.
    pub plus_dirs: bool,
}

/// Encapsulates a command completion specification; provides policy for how to
/// generate completions for a given input.
#[derive(Clone, Debug, Default)]
pub struct Spec {
    //
    // Options
    /// Options to use for completion.
    pub options: GenerationOptions,

    //
    // Generators
    /// Actions to take to generate completions.
    pub actions: Vec<CompleteAction>,
    /// Optionally, a glob pattern whose expansion will be used as completions.
    pub glob_pattern: Option<String>,
    /// Optionally, a list of words to use as completions.
    pub word_list: Option<String>,
    /// Optionally, the name of a shell function to invoke to generate completions.
    pub function_name: Option<String>,
    /// Optionally, the name of a command to execute to generate completions.
    pub command: Option<String>,

    //
    // Filters
    /// Optionally, a pattern to filter completions.
    pub filter_pattern: Option<String>,
    /// If true, completion candidates matching `filter_pattern` are removed;
    /// otherwise, those not matching it are removed.
    pub filter_pattern_excludes: bool,

    //
    // Transformers
    /// Optionally, provides a prefix to be prepended to all completion candidates.
    pub prefix: Option<String>,
    /// Optionally, provides a suffix to be prepended to all completion candidates.
    pub suffix: Option<String>,
}

/// Encapsulates context used during completion generation.
#[derive(Debug)]
pub struct Context<'a> {
    /// The token to complete.
    pub token_to_complete: &'a str,

    /// If available, the name of the command being invoked.
    pub command_name: Option<&'a str>,
    /// If there was one, the token preceding the one being completed.
    pub preceding_token: Option<&'a str>,

    /// The 0-based index of the token to complete.
    pub token_index: usize,

    /// The input line.
    pub input_line: &'a str,
    /// The 0-based index of the cursor in the input line.
    pub cursor_index: usize,
    /// The tokens in the input line.
    pub tokens: &'a [&'a brush_parser::Token],
}

impl Spec {
    /// Generates completion candidates using this specification.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance to use for completion generation.
    /// * `context` - The context in which completion is being generated.
    #[expect(clippy::too_many_lines)]
    pub async fn get_completions(
        &self,
        shell: &mut Shell,
        context: &Context<'_>,
    ) -> Result<Answer, crate::error::Error> {
        // Store the current options in the shell; this is needed since the compopt
        // built-in has the ability of modifying the options for an in-flight
        // completion process.
        shell.completion_config.current_completion_options = Some(self.options.clone());

        // Generate completions based on any provided actions (and on words).
        let mut candidates = self.generate_action_completions(shell, context).await?;
        if let Some(word_list) = &self.word_list {
            let params = shell.default_exec_params();
            let words =
                crate::expansion::full_expand_and_split_str(shell, &params, word_list).await?;
            for word in words {
                if word.starts_with(context.token_to_complete) {
                    candidates.insert(word);
                }
            }
        }

        if let Some(glob_pattern) = &self.glob_pattern {
            let pattern = patterns::Pattern::from(glob_pattern.as_str())
                .set_extended_globbing(shell.options.extended_globbing)
                .set_case_insensitive(shell.options.case_insensitive_pathname_expansion);

            let expansions = pattern.expand(
                shell.working_dir(),
                Some(&patterns::Pattern::accept_all_expand_filter),
                &patterns::FilenameExpansionOptions::default(),
            )?;

            for expansion in expansions {
                candidates.insert(expansion);
            }
        }
        if let Some(function_name) = &self.function_name {
            let call_result = self
                .call_completion_function(shell, function_name.as_str(), context)
                .await?;

            match call_result {
                Answer::RestartCompletionProcess => return Ok(call_result),
                Answer::Candidates(mut new_candidates, _options) => {
                    candidates.append(&mut new_candidates);
                }
            }
        }
        if let Some(command) = &self.command {
            let mut new_candidates = self
                .call_completion_command(shell, command.as_str(), context)
                .await?;
            candidates.append(&mut new_candidates);
        }

        // Apply filter pattern, if present. Anything the filter selects gets removed.
        if let Some(filter_pattern) = &self.filter_pattern {
            if !filter_pattern.is_empty() {
                let mut updated = IndexSet::new();

                for candidate in candidates {
                    let matches = completion_filter_pattern_matches(
                        filter_pattern.as_str(),
                        candidate.as_str(),
                        context.token_to_complete,
                        shell,
                    )?;

                    if self.filter_pattern_excludes != matches {
                        updated.insert(candidate);
                    }
                }

                candidates = updated;
            }
        }

        // Add prefix and/or suffix, if present.
        if self.prefix.is_some() || self.suffix.is_some() {
            let empty = String::new();
            let prefix = self.prefix.as_ref().unwrap_or(&empty);
            let suffix = self.suffix.as_ref().unwrap_or(&empty);

            let mut updated = IndexSet::new();
            for candidate in candidates {
                updated.insert(std::format!("{prefix}{candidate}{suffix}"));
            }

            candidates = updated;
        }

        //
        // Now apply options
        //

        let options = if let Some(options) = &shell.completion_config.current_completion_options {
            options
        } else {
            &self.options
        };

        let processing_options = ProcessingOptions {
            treat_as_filenames: options.file_names,
            no_autoquote_filenames: options.no_quote,
            no_trailing_space_at_end_of_line: options.no_space,
        };

        if options.plus_dirs {
            // Also add dir name completion.
            let mut dir_candidates = get_file_completions(
                shell,
                context.token_to_complete,
                /* must_be_dir */ true,
            )
            .await;
            candidates.append(&mut dir_candidates);
        }

        // If we still haven't found any completion candidates by now, then consider whether any
        // requests were made for fallbacks.
        if candidates.is_empty() {
            if options.bash_default {
                //
                // TODO: if we have no completions, then fall back to default "bash" completions.
                // It's not clear what exactly this means, though. From basic testing, it doesn't
                // seem to include basic file and directory name completion.
                //
                tracing::debug!(target: trace_categories::COMPLETION, "unimplemented: complete -o bashdefault");
            }
            if options.default || options.dir_names {
                // N.B. We approximate "default" readline completion behavior by getting file and
                // dir completions.
                let must_be_dir = options.dir_names;

                let mut default_candidates =
                    get_file_completions(shell, context.token_to_complete, must_be_dir).await;
                candidates.append(&mut default_candidates);
            }
        }

        // Sort, unless blocked by options.
        if !self.options.no_sort {
            candidates.sort();
        }

        Ok(Answer::Candidates(candidates, processing_options))
    }

    #[expect(clippy::too_many_lines)]
    async fn generate_action_completions(
        &self,
        shell: &Shell,
        context: &Context<'_>,
    ) -> Result<IndexSet<String>, error::Error> {
        let mut candidates = IndexSet::new();

        let token = context.token_to_complete;

        for action in &self.actions {
            match action {
                CompleteAction::Alias => {
                    for name in shell.aliases.keys() {
                        if name.starts_with(token) {
                            candidates.insert(name.clone());
                        }
                    }
                }
                CompleteAction::ArrayVar => {
                    for (name, var) in shell.env.iter() {
                        if var.value().is_array() && name.starts_with(token) {
                            candidates.insert(name.to_owned());
                        }
                    }
                }
                CompleteAction::Binding => {
                    tracing::debug!(target: trace_categories::COMPLETION, "unimplemented: complete -A binding");
                }
                CompleteAction::Builtin => {
                    for name in shell.builtins().keys() {
                        if name.starts_with(token) {
                            candidates.insert(name.to_owned());
                        }
                    }
                }
                CompleteAction::Command => {
                    let mut command_completions = get_command_completions(shell, context);
                    candidates.append(&mut command_completions);
                }
                CompleteAction::Directory => {
                    let mut file_completions =
                        get_file_completions(shell, context.token_to_complete, true).await;
                    candidates.append(&mut file_completions);
                }
                CompleteAction::Disabled => {
                    for (name, registration) in shell.builtins() {
                        if registration.disabled && name.starts_with(token) {
                            candidates.insert(name.to_owned());
                        }
                    }
                }
                CompleteAction::Enabled => {
                    for (name, registration) in shell.builtins() {
                        if !registration.disabled && name.starts_with(token) {
                            candidates.insert(name.to_owned());
                        }
                    }
                }
                CompleteAction::Export => {
                    for (key, value) in shell.env.iter() {
                        if value.is_exported() && key.starts_with(token) {
                            candidates.insert(key.to_owned());
                        }
                    }
                }
                CompleteAction::File => {
                    let mut file_completions =
                        get_file_completions(shell, context.token_to_complete, false).await;
                    candidates.append(&mut file_completions);
                }
                CompleteAction::Function => {
                    for (name, _) in shell.funcs().iter() {
                        candidates.insert(name.to_owned());
                    }
                }
                CompleteAction::Group => {
                    for group_name in users::get_all_groups()? {
                        if group_name.starts_with(token) {
                            candidates.insert(group_name);
                        }
                    }
                }
                CompleteAction::HelpTopic => {
                    // For now, we only have help topics for built-in commands.
                    for name in shell.builtins().keys() {
                        if name.starts_with(token) {
                            candidates.insert(name.to_owned());
                        }
                    }
                }
                CompleteAction::HostName => {
                    // N.B. We only retrieve one hostname.
                    if let Ok(name) = sys::network::get_hostname() {
                        let name = name.to_string_lossy();
                        if name.starts_with(token) {
                            candidates.insert(name.to_string());
                        }
                    }
                }
                CompleteAction::Job => {
                    for job in &shell.jobs.jobs {
                        let command_name = job.command_name();
                        if command_name.starts_with(token) {
                            candidates.insert(command_name.to_owned());
                        }
                    }
                }
                CompleteAction::Keyword => {
                    for keyword in shell.get_keywords() {
                        if keyword.starts_with(token) {
                            candidates.insert(keyword.clone());
                        }
                    }
                }
                CompleteAction::Running => {
                    for job in &shell.jobs.jobs {
                        if matches!(job.state, jobs::JobState::Running) {
                            let command_name = job.command_name();
                            if command_name.starts_with(token) {
                                candidates.insert(command_name.to_owned());
                            }
                        }
                    }
                }
                CompleteAction::Service => {
                    tracing::debug!(target: trace_categories::COMPLETION, "unimplemented: complete -A service");
                }
                CompleteAction::SetOpt => {
                    for option in namedoptions::options(namedoptions::ShellOptionKind::SetO).iter()
                    {
                        if option.name.starts_with(token) {
                            candidates.insert(option.name.to_owned());
                        }
                    }
                }
                CompleteAction::ShOpt => {
                    for option in namedoptions::options(namedoptions::ShellOptionKind::Shopt).iter()
                    {
                        if option.name.starts_with(token) {
                            candidates.insert(option.name.to_owned());
                        }
                    }
                }
                CompleteAction::Signal => {
                    for signal in traps::TrapSignal::iterator() {
                        if signal.as_str().starts_with(token) {
                            candidates.insert(signal.as_str().to_string());
                        }
                    }
                }
                CompleteAction::Stopped => {
                    for job in &shell.jobs.jobs {
                        if matches!(job.state, jobs::JobState::Stopped) {
                            let command_name = job.command_name();
                            if command_name.starts_with(token) {
                                candidates.insert(job.command_name().to_owned());
                            }
                        }
                    }
                }
                CompleteAction::User => {
                    for user_name in users::get_all_users()? {
                        if user_name.starts_with(token) {
                            candidates.insert(user_name);
                        }
                    }
                }
                CompleteAction::Variable => {
                    for (key, _) in shell.env.iter() {
                        if key.starts_with(token) {
                            candidates.insert(key.to_owned());
                        }
                    }
                }
            }
        }

        Ok(candidates)
    }

    async fn call_completion_command(
        &self,
        shell: &Shell,
        command_name: &str,
        context: &Context<'_>,
    ) -> Result<IndexSet<String>, error::Error> {
        // Move to a subshell so we can start filling out variables.
        let mut shell = shell.clone();

        let vars_and_values: Vec<(&str, ShellValueLiteral)> = vec![
            ("COMP_LINE", context.input_line.into()),
            ("COMP_POINT", context.cursor_index.to_string().into()),
            // TODO: add COMP_KEY
            // TODO: add COMP_TYPE
        ];

        // Fill out variables.
        for (var, value) in vars_and_values {
            shell.env.update_or_add(
                var,
                value,
                |v| {
                    v.export();
                    Ok(())
                },
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        }

        // Compute args.
        let mut args = vec![
            context.command_name.unwrap_or(""),
            context.token_to_complete,
        ];
        if let Some(preceding_token) = context.preceding_token {
            args.push(preceding_token);
        }

        // Compose the full command line.
        let mut command_line = command_name.to_owned();
        for arg in args {
            command_line.push(' ');

            let escaped_arg = escape::quote_if_needed(arg, escape::QuoteMode::SingleQuote);
            command_line.push_str(escaped_arg.as_ref());
        }

        // Run the command.
        let params = shell.default_exec_params();
        let output =
            commands::invoke_command_in_subshell_and_get_output(&mut shell, &params, command_line)
                .await?;

        // Split results.
        let mut candidates = IndexSet::new();
        for line in output.lines() {
            candidates.insert(line.to_owned());
        }

        Ok(candidates)
    }

    async fn call_completion_function(
        &self,
        shell: &mut Shell,
        function_name: &str,
        context: &Context<'_>,
    ) -> Result<Answer, error::Error> {
        // TODO: Don't pollute the persistent environment with these?
        let vars_and_values: Vec<(&str, ShellValueLiteral)> = vec![
            ("COMP_LINE", context.input_line.into()),
            ("COMP_POINT", context.cursor_index.to_string().into()),
            // TODO: add COMP_KEY
            // TODO: add COMP_TYPE
            (
                "COMP_WORDS",
                context
                    .tokens
                    .iter()
                    .map(|t| t.to_str())
                    .collect::<Vec<_>>()
                    .into(),
            ),
            ("COMP_CWORD", context.token_index.to_string().into()),
        ];

        tracing::debug!(target: trace_categories::COMPLETION, "[calling completion func '{function_name}']: {}",
            vars_and_values.iter().map(|(k, v)| std::format!("{k}={v}")).collect::<Vec<String>>().join(" "));

        let mut vars_to_remove = vec![];
        for (var, value) in vars_and_values {
            shell.env.update_or_add(
                var,
                value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;

            vars_to_remove.push(var);
        }

        let mut args = vec![
            context.command_name.unwrap_or(""),
            context.token_to_complete,
        ];
        if let Some(preceding_token) = context.preceding_token {
            args.push(preceding_token);
        }

        // TODO: Find a more appropriate interlock here. For now we use the existing
        // handler depth count to suppress any debug traps.
        shell.traps.handler_depth += 1;

        let params = shell.default_exec_params();
        let invoke_result = shell
            .invoke_function(function_name, args.iter(), &params)
            .await;
        tracing::debug!(target: trace_categories::COMPLETION, "[completion function '{function_name}' returned: {invoke_result:?}]");

        shell.traps.handler_depth -= 1;

        // Make a best-effort attempt to unset the temporary variables.
        for var_name in vars_to_remove {
            let _ = shell.env.unset(var_name);
        }

        let result = invoke_result.unwrap_or_else(|e| {
            tracing::warn!(target: trace_categories::COMPLETION, "error while running completion function '{function_name}': {e}");
            1 // Report back a non-zero exit code.
        });

        // When the function returns the special value 124, then it's a request
        // for us to restart the completion process.
        if result == 124 {
            Ok(Answer::RestartCompletionProcess)
        } else {
            if let Some(reply) = shell.env.unset("COMPREPLY")? {
                tracing::debug!(target: trace_categories::COMPLETION, "[completion function yielded: {reply:?}]");

                match reply.value() {
                    variables::ShellValue::IndexedArray(values) => {
                        return Ok(Answer::Candidates(
                            values.values().map(|v| v.to_owned()).collect(),
                            ProcessingOptions::default(),
                        ));
                    }
                    variables::ShellValue::String(s) => {
                        let mut candidates = IndexSet::new();
                        candidates.insert(s.to_owned());

                        return Ok(Answer::Candidates(candidates, ProcessingOptions::default()));
                    }
                    _ => (),
                }
            }

            Ok(Answer::Candidates(
                IndexSet::new(),
                ProcessingOptions::default(),
            ))
        }
    }
}

/// Represents a set of generated command completions.
#[derive(Debug, Default)]
pub struct Completions {
    /// The index in the input line where the completions should be inserted. Represented
    /// as a byte offset into the input line; must be at a clean character boundary.
    pub insertion_index: usize,
    /// The number of elements in the input line that should be removed before insertion.
    /// Represented as a byte count; must capture an exact character boundary.
    pub delete_count: usize,
    /// The ordered set of completions.
    pub candidates: IndexSet<String>,
    /// Options for processing the candidates.
    pub options: ProcessingOptions,
}

/// Options governing how command completion candidates are processed after being generated.
#[derive(Debug)]
pub struct ProcessingOptions {
    /// Treat completions as file names.
    pub treat_as_filenames: bool,
    /// Don't auto-quote completions that are file names.
    pub no_autoquote_filenames: bool,
    /// Don't append a trailing space to completions at the end of the input line.
    pub no_trailing_space_at_end_of_line: bool,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        Self {
            treat_as_filenames: true,
            no_autoquote_filenames: false,
            no_trailing_space_at_end_of_line: false,
        }
    }
}

/// Encapsulates a completion answer.
pub enum Answer {
    /// The completion process generated a set of candidates along with options
    /// controlling how to process them.
    Candidates(IndexSet<String>, ProcessingOptions),
    /// The completion process needs to be restarted.
    RestartCompletionProcess,
}

const EMPTY_COMMAND: &str = "_EmptycmD_";
const DEFAULT_COMMAND: &str = "_DefaultCmD_";
const INITIAL_WORD: &str = "_InitialWorD_";

impl Config {
    /// Removes all registered completion specs.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.empty_line = None;
        self.default = None;
        self.initial_word = None;
    }

    /// Ensures the named completion spec is no longer registered; returns whether a
    /// removal operation was required.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the completion spec to remove.
    pub fn remove(&mut self, name: &str) -> bool {
        match name {
            EMPTY_COMMAND => {
                let result = self.empty_line.is_some();
                self.empty_line = None;
                result
            }
            DEFAULT_COMMAND => {
                let result = self.default.is_some();
                self.default = None;
                result
            }
            INITIAL_WORD => {
                let result = self.initial_word.is_some();
                self.initial_word = None;
                result
            }
            _ => self.commands.remove(name).is_some(),
        }
    }

    /// Returns an iterator over the completion specs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Spec)> {
        self.commands.iter()
    }

    /// If present, returns the completion spec for the command of the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    pub fn get(&self, name: &str) -> Option<&Spec> {
        match name {
            EMPTY_COMMAND => self.empty_line.as_ref(),
            DEFAULT_COMMAND => self.default.as_ref(),
            INITIAL_WORD => self.initial_word.as_ref(),
            _ => self.commands.get(name),
        }
    }

    /// If present, sets the provided completion spec to be associated with the
    /// command of the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    /// * `spec` - The completion spec to associate with the command.
    pub fn set(&mut self, name: &str, spec: Spec) {
        match name {
            EMPTY_COMMAND => {
                self.empty_line = Some(spec);
            }
            DEFAULT_COMMAND => {
                self.default = Some(spec);
            }
            INITIAL_WORD => {
                self.initial_word = Some(spec);
            }
            _ => {
                self.commands.insert(name.to_owned(), spec);
            }
        }
    }

    /// Returns a mutable reference to the completion spec for the command of the
    /// given name; if the command already was associated with a spec, returns
    /// a reference to that existing spec. Otherwise registers a new default
    /// spec and returns a mutable reference to it.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    pub fn get_or_add_mut(&mut self, name: &str) -> &mut Spec {
        match name {
            EMPTY_COMMAND => {
                if self.empty_line.is_none() {
                    self.empty_line = Some(Spec::default());
                }
                self.empty_line.as_mut().unwrap()
            }
            DEFAULT_COMMAND => {
                if self.default.is_none() {
                    self.default = Some(Spec::default());
                }
                self.default.as_mut().unwrap()
            }
            INITIAL_WORD => {
                if self.initial_word.is_none() {
                    self.initial_word = Some(Spec::default());
                }
                self.initial_word.as_mut().unwrap()
            }
            _ => self.commands.entry(name.to_owned()).or_default(),
        }
    }

    /// Generates completions for the given input line and cursor position.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance to use for completion generation.
    /// * `input` - The input line for which completions are being generated.
    /// * `position` - The 0-based index of the cursor in the input line.
    #[expect(clippy::string_slice)]
    pub async fn get_completions(
        &self,
        shell: &mut Shell,
        input: &str,
        position: usize,
    ) -> Result<Completions, error::Error> {
        const MAX_RESTARTS: u32 = 10;

        // Make a best-effort attempt to tokenize.
        let tokens = Self::tokenize_input_for_completion(shell, input);

        let cursor = position;
        let mut preceding_token = None;
        let mut completion_prefix = "";
        let mut insertion_index = cursor;
        let mut completion_token_index = tokens.len();

        // Copy a set of references to the tokens; we will adjust this list as
        // we find we need to insert an empty token.
        let mut adjusted_tokens: Vec<&brush_parser::Token> = tokens.iter().collect();

        // Try to find which token we are in.
        for (i, token) in tokens.iter().enumerate() {
            // If the cursor is before the start of the token, then it's between
            // this token and the one that preceded it (or it's before the first
            // token if this is the first token).
            if cursor < token.location().start.index {
                // TODO: Should insert an empty token here; the position looks to have
                // been between this token and the preceding one.
                completion_token_index = i;
                break;
            }
            // If the cursor is anywhere from the first char of the token up to
            // (and including) the first char after the token, then this we need
            // to generate completions to replace/update this token. We'll pay
            // attention to the position to figure out the prefix that we should
            // be completing.
            else if cursor >= token.location().start.index && cursor <= token.location().end.index
            {
                // Update insertion index.
                insertion_index = token.location().start.index;

                // Update prefix.
                let offset_into_token = cursor - insertion_index;
                let token_str = token.to_str();
                completion_prefix = &token_str[..offset_into_token];

                // Update token index.
                completion_token_index = i;

                break;
            }

            // Otherwise, we need to keep looking. Update what we think the
            // preceding token may be.
            preceding_token = Some(token);
        }

        // If the position is after the last token, then we need to insert an empty
        // token for the new token to be generated.
        let empty_token =
            brush_parser::Token::Word(String::new(), brush_parser::TokenLocation::default());
        if completion_token_index == tokens.len() {
            adjusted_tokens.push(&empty_token);
        }

        // Get the completions.
        let mut result = Answer::RestartCompletionProcess;
        let mut restart_count = 0;
        while matches!(result, Answer::RestartCompletionProcess) {
            if restart_count > MAX_RESTARTS {
                tracing::warn!("possible infinite loop detected in completion process");
                break;
            }

            let completion_context = Context {
                token_to_complete: completion_prefix,
                preceding_token: preceding_token.map(|t| t.to_str()),
                command_name: adjusted_tokens.first().map(|token| token.to_str()),
                input_line: input,
                token_index: completion_token_index,
                tokens: adjusted_tokens.as_slice(),
                cursor_index: position,
            };

            result = self
                .get_completions_for_token(shell, completion_context)
                .await;

            restart_count += 1;
        }

        match result {
            Answer::Candidates(candidates, options) => Ok(Completions {
                insertion_index,
                delete_count: completion_prefix.len(),
                candidates,
                options,
            }),
            Answer::RestartCompletionProcess => Ok(Completions {
                insertion_index,
                delete_count: 0,
                candidates: IndexSet::new(),
                options: ProcessingOptions::default(),
            }),
        }
    }

    fn tokenize_input_for_completion(shell: &Shell, input: &str) -> Vec<brush_parser::Token> {
        const FALLBACK: &str = " \t\n\"\'@><=;|&(:";

        let delimiter_str = shell
            .env_str("COMP_WORDBREAKS")
            .unwrap_or_else(|| FALLBACK.into());

        let delimiters: Vec<_> = delimiter_str.chars().collect();

        simple_tokenize_by_delimiters(input, delimiters.as_slice())
    }

    async fn get_completions_for_token(&self, shell: &mut Shell, context: Context<'_>) -> Answer {
        // See if we can find a completion spec matching the current command.
        let mut found_spec: Option<&Spec> = None;

        if let Some(command_name) = context.command_name {
            if context.token_index == 0 {
                if let Some(spec) = &self.initial_word {
                    found_spec = Some(spec);
                }
            } else {
                if let Some(spec) = shell.completion_config.commands.get(command_name) {
                    found_spec = Some(spec);
                } else if let Some(file_name) = PathBuf::from(command_name).file_name() {
                    if let Some(spec) = shell
                        .completion_config
                        .commands
                        .get(&file_name.to_string_lossy().to_string())
                    {
                        found_spec = Some(spec);
                    }
                }

                if found_spec.is_none() {
                    if let Some(spec) = &self.default {
                        found_spec = Some(spec);
                    }
                }
            }
        } else {
            if let Some(spec) = &self.empty_line {
                found_spec = Some(spec);
            }
        }

        // Try to generate completions.
        if let Some(spec) = found_spec {
            spec.to_owned()
                .get_completions(shell, &context)
                .await
                .unwrap_or_else(|_err| {
                    Answer::Candidates(IndexSet::new(), ProcessingOptions::default())
                })
        } else {
            // If we didn't find a spec, then fall back to basic completion.
            get_completions_using_basic_lookup(shell, &context).await
        }
    }
}

async fn get_file_completions(
    shell: &Shell,
    token_to_complete: &str,
    must_be_dir: bool,
) -> IndexSet<String> {
    // Basic-expand the token-to-be-completed; it won't have been expanded to this point.
    let mut throwaway_shell = shell.clone();
    let params = throwaway_shell.default_exec_params();
    let expanded_token = throwaway_shell
        .basic_expand_string(&params, token_to_complete)
        .await
        .unwrap_or_else(|_err| token_to_complete.to_owned());

    let glob = std::format!("{expanded_token}*");

    let path_filter = |path: &Path| !must_be_dir || shell.absolute_path(path).is_dir();

    let pattern = patterns::Pattern::from(glob)
        .set_extended_globbing(shell.options.extended_globbing)
        .set_case_insensitive(shell.options.case_insensitive_pathname_expansion);

    pattern
        .expand(
            shell.working_dir(),
            Some(&path_filter),
            &patterns::FilenameExpansionOptions::default(),
        )
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn get_command_completions(shell: &Shell, context: &Context<'_>) -> IndexSet<String> {
    let mut candidates = IndexSet::new();

    // Look for external commands.
    for path in shell.find_executables_in_path_with_prefix(
        context.token_to_complete,
        shell.options.case_insensitive_pathname_expansion,
    ) {
        if let Some(file_name) = path.file_name() {
            candidates.insert(file_name.to_string_lossy().to_string());
        }
    }

    candidates.into_iter().collect()
}

async fn get_completions_using_basic_lookup(shell: &Shell, context: &Context<'_>) -> Answer {
    let mut candidates = get_file_completions(shell, context.token_to_complete, false).await;

    // If this appears to be the command token (and if there's *some* prefix without
    // a path separator) then also consider whether we should search the path for
    // completions too.
    // TODO: Do a better job than just checking if index == 0.
    if context.token_index == 0
        && !context.token_to_complete.is_empty()
        && !context
            .token_to_complete
            .contains(std::path::MAIN_SEPARATOR)
    {
        // Add external commands.
        let mut command_completions = get_command_completions(shell, context);
        candidates.append(&mut command_completions);

        // Add built-in commands.
        for (name, registration) in shell.builtins() {
            if !registration.disabled && name.starts_with(context.token_to_complete) {
                candidates.insert(name.to_owned());
            }
        }

        // Add shell functions.
        for (name, _) in shell.funcs().iter() {
            if name.starts_with(context.token_to_complete) {
                candidates.insert(name.to_owned());
            }
        }

        // Add aliases.
        for name in shell.aliases.keys() {
            if name.starts_with(context.token_to_complete) {
                candidates.insert(name.to_owned());
            }
        }

        // Add keywords.
        for keyword in shell.get_keywords() {
            if keyword.starts_with(context.token_to_complete) {
                candidates.insert(keyword.clone());
            }
        }

        // Sort.
        candidates.sort();
    }

    #[cfg(windows)]
    {
        candidates = candidates
            .into_iter()
            .map(|c| c.replace('\\', "/"))
            .collect();
    }

    Answer::Candidates(candidates, ProcessingOptions::default())
}

fn simple_tokenize_by_delimiters(input: &str, delimiters: &[char]) -> Vec<brush_parser::Token> {
    //
    // This is an overly naive tokenization.
    //

    let mut tokens = vec![];
    let mut start = 0;

    for piece in input.split_inclusive(delimiters) {
        let next_start = start + piece.len();

        let piece = piece.strip_suffix(delimiters).unwrap_or(piece);
        let end = start + piece.len();
        tokens.push(brush_parser::Token::Word(
            piece.to_string(),
            brush_parser::TokenLocation {
                start: brush_parser::SourcePosition {
                    index: start,
                    line: 1,
                    column: start + 1,
                }
                .into(),
                end: brush_parser::SourcePosition {
                    index: end,
                    line: 1,
                    column: end + 1,
                }
                .into(),
            },
        ));

        start = next_start;
    }

    tokens
}

fn completion_filter_pattern_matches(
    pattern: &str,
    candidate: &str,
    token_being_completed: &str,
    shell: &Shell,
) -> Result<bool, error::Error> {
    let pattern = replace_unescaped_ampersands(pattern, token_being_completed);

    //
    // TODO: Replace unescaped '&' with the word being completed.
    //

    let pattern = patterns::Pattern::from(pattern.as_ref())
        .set_extended_globbing(shell.options.extended_globbing)
        .set_case_insensitive(shell.options.case_insensitive_pathname_expansion);

    let matches = pattern.exactly_matches(candidate)?;

    Ok(matches)
}

fn replace_unescaped_ampersands<'a>(pattern: &'a str, replacement: &str) -> Cow<'a, str> {
    let mut in_escape = false;
    let mut insertion_points = vec![];

    for (i, c) in pattern.char_indices() {
        if !in_escape && c == '&' {
            insertion_points.push(i);
        }
        in_escape = !in_escape && c == '\\';
    }

    if insertion_points.is_empty() {
        return pattern.into();
    }

    let mut result = pattern.to_owned();
    for i in insertion_points.iter().rev() {
        result.replace_range(*i..=*i, replacement);
    }

    result.into()
}
