//! Implements programmable command completion support.

use itertools::Itertools;
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
};
use strum::IntoEnumIterator;

use crate::{
    Shell, commands, env, error, escape, expansion, extensions, interfaces, jobs, namedoptions,
    patterns,
    sys::{self, users},
    trace_categories, traps,
    variables::{self, ShellValueLiteral},
};
use brush_parser::unquote_str;

// `compgen -W` splits unquoted literal IFS characters before expanding each resulting word.
fn split_completion_word_list(
    word_list: &str,
    ifs: &str,
    parser_options: &brush_parser::ParserOptions,
) -> Result<Vec<String>, error::Error> {
    let pieces = brush_parser::word::parse(word_list, parser_options)?;
    let mut words = vec![];
    let mut current_word = String::new();

    for piece in pieces {
        let source = word_list
            .get(piece.start_index..piece.end_index)
            .ok_or_else(|| {
                error::ErrorKind::InternalError(String::from(
                    "word parser returned an invalid source span",
                ))
            })?;

        if matches!(piece.piece, brush_parser::word::WordPiece::Text(_)) {
            for c in source.chars() {
                if ifs.contains(c) {
                    if !current_word.is_empty() {
                        words.push(std::mem::take(&mut current_word));
                    }
                } else {
                    current_word.push(c);
                }
            }
        } else {
            current_word.push_str(source);
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    Ok(words)
}

/// Type of action to take to generate completion candidates. Each one's name (e.g.
/// `arrayvar`) is what `complete -A` takes.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    strum_macros::EnumIter,
    strum_macros::EnumMessage,
    strum_macros::EnumString,
    strum_macros::IntoStaticStr,
)]
#[strum(serialize_all = "lowercase")]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CompleteAction {
    /// Complete with valid aliases.
    Alias,
    /// Complete with names of array shell variables.
    ArrayVar,
    /// Complete with names of key bindings.
    Binding,
    /// Complete with names of shell builtins.
    Builtin,
    /// Complete with names of executable commands.
    Command,
    /// Complete with directory names.
    Directory,
    /// Complete with names of disabled shell builtins.
    Disabled,
    /// Complete with names of enabled shell builtins.
    Enabled,
    /// Complete with names of exported shell variables.
    Export,
    /// Complete with filenames.
    File,
    /// Complete with names of shell functions.
    Function,
    /// Complete with valid user groups.
    Group,
    /// Complete with names of valid shell help topics.
    HelpTopic,
    /// Complete with the system's hostname(s).
    HostName,
    /// Complete with the command names of shell-managed jobs.
    Job,
    /// Complete with valid shell keywords.
    Keyword,
    /// Complete with the command names of running shell-managed jobs.
    Running,
    /// Complete with names of system services.
    Service,
    /// Complete with the names of options settable via set -o.
    SetOpt,
    /// Complete with the names of options settable via shopt.
    ShOpt,
    /// Complete with the names of trappable signals.
    Signal,
    /// Complete with the command names of stopped shell-managed jobs.
    Stopped,
    /// Complete with valid usernames.
    User,
    /// Complete with names of shell variables.
    Variable,
}

/// Options influencing how command completions are generated. Each one's name (e.g.
/// `nospace`) is what `complete -o` takes; they're declared in the order bash lists them.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    strum_macros::EnumIter,
    strum_macros::EnumMessage,
    strum_macros::EnumString,
    strum_macros::IntoStaticStr,
)]
#[strum(serialize_all = "lowercase")]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CompleteOption {
    /// Perform rest of default completions if no completions are generated.
    BashDefault,
    /// Use default filename completion if no completions are generated.
    Default,
    /// Treat completions as directory names.
    DirNames,
    /// Treat completions as filenames.
    FileNames,
    /// Suppress default auto-quotation of completions.
    NoQuote,
    /// Do not sort completions.
    NoSort,
    /// Do not append a trailing space to completions at the end of the input line.
    NoSpace,
    /// Also generate directory completions.
    PlusDirs,
}

/// Options for generating completions: which [`CompleteOption`]s are enabled.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GenerationOptions {
    enabled: BTreeSet<CompleteOption>,
}

impl FromIterator<CompleteOption> for GenerationOptions {
    /// Returns options with just the given ones enabled.
    fn from_iter<I: IntoIterator<Item = CompleteOption>>(options: I) -> Self {
        Self {
            enabled: options.into_iter().collect(),
        }
    }
}

impl GenerationOptions {
    /// Returns whether `option` is enabled.
    pub fn get(&self, option: CompleteOption) -> bool {
        self.enabled.contains(&option)
    }

    /// Enables or disables `option`.
    pub fn set(&mut self, option: CompleteOption, enabled: bool) {
        if enabled {
            self.enabled.insert(option);
        } else {
            self.enabled.remove(&option);
        }
    }
}

/// Encapsulates a command completion specification; provides policy for how to
/// generate completions for a given input.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// Optionally, a pattern to filter completions (`complete -X`), as given: candidates
    /// it matches are removed, or, if it starts with a `!` (that doesn't start an extglob
    /// pattern), those it doesn't match.
    pub filter_pattern: Option<String>,

    //
    // Transformers
    /// Optionally, provides a prefix to be prepended to all completion candidates.
    pub prefix: Option<String>,
    /// Optionally, provides a suffix to be appended to all completion candidates.
    pub suffix: Option<String>,
}

/// Encapsulates the shell's programmable command completion configuration.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    /// The specs for completing commands' arguments, by command name.
    commands: HashMap<String, Spec>,
    /// The specs used in place of a command's.
    specials: HashMap<SpecialSpec, Spec>,

    /// Optionally, stores the current completion options in effect. May be mutated
    /// while a completion generation is in-flight.
    pub current_completion_options: Option<GenerationOptions>,

    /// Fallback options to use when 'default' completions are requested (not to be
    /// confused with the 'default' completion spec, nor 'bashdefault' completions).
    pub fallback_options: FallbackOptions,
}

/// Options for fallback completions.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FallbackOptions {
    /// If true, mark directory completions with a trailing slash.
    pub mark_directories: bool,
    /// If true, mark symlinked directory completions with a trailing slash.
    pub mark_symlinked_directories: bool,
}

impl Default for FallbackOptions {
    fn default() -> Self {
        Self {
            mark_directories: true,
            mark_symlinked_directories: false,
        }
    }
}

/// Names a completion spec in a [`Config`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SpecName<'a> {
    /// The spec for completing the named command's arguments.
    Command(&'a str),
    /// One of the special specs.
    Special(SpecialSpec),
}

impl<'a> SpecName<'a> {
    /// Returns the spec named `name`: a command's, or the special spec that bash's name for
    /// it (see [`SpecialSpec::command_name`]) names.
    pub fn parse(name: &'a str) -> Self {
        SpecialSpec::from_command_name(name).map_or(Self::Command(name), Self::Special)
    }

    /// Returns the name: a command's, or for a special spec, bash's name for it.
    pub const fn as_str(self) -> &'a str {
        match self {
            Self::Command(command) => command,
            Self::Special(special) => special.command_name(),
        }
    }
}

/// A completion spec used in place of a command's.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, strum_macros::EnumIter)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SpecialSpec {
    /// The spec used when no command's spec applies (`complete -D`).
    Default,
    /// The spec used when the command line is empty (`complete -E`).
    EmptyLine,
    /// The spec used for the initial word of a command line (`complete -I`).
    InitialWord,
}

impl SpecialSpec {
    /// Returns the name that stands in for a command's with this spec, as bash's does:
    /// the `complete` and `compopt` builtins accept it in place of a command name.
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Default => "_DefaultCmD_",
            Self::EmptyLine => "_EmptycmD_",
            Self::InitialWord => "_InitialWorD_",
        }
    }

    /// Returns the special spec that `name`, bash's name for it, names, if any.
    pub fn from_command_name(name: &str) -> Option<Self> {
        Self::iter().find(|special| special.command_name() == name)
    }
}

/// Describes what triggered the completion process.
#[derive(Clone, Copy, Debug, Default)]
pub enum CompletionTrigger {
    /// Interactive completion triggered by Tab key (normal completion).
    #[default]
    InteractiveComplete,
    /// Programmatic generation via the `compgen` builtin.
    Programmatic,
}

impl CompletionTrigger {
    /// Returns the `COMP_TYPE` value for this trigger.
    pub const fn comp_type(self) -> i32 {
        match self {
            Self::InteractiveComplete => 9, // TAB = normal completion
            Self::Programmatic => 0,
        }
    }

    /// Returns the `COMP_KEY` value for this trigger.
    pub const fn comp_key(self) -> i32 {
        match self {
            Self::InteractiveComplete => 9, // TAB key
            Self::Programmatic => 0,
        }
    }
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
    pub tokens: &'a [&'a CompletionToken<'a>],

    /// What triggered the completion.
    pub trigger: CompletionTrigger,
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
        shell: &mut Shell<impl extensions::ShellExtensions>,
        context: &Context<'_>,
    ) -> Result<Answer, crate::error::Error> {
        // Store the current options in the shell; this is needed since the compopt
        // built-in has the ability of modifying the options for an in-flight
        // completion process.
        shell.completion_config_mut().current_completion_options = Some(self.options.clone());

        // Generate completions based on any provided actions (and on words).
        let mut candidates = self.generate_action_completions(shell, context).await?;
        if let Some(word_list) = &self.word_list {
            let params = shell.default_exec_params();
            let unexpanded_words =
                split_completion_word_list(word_list, &shell.ifs(), &shell.parser_options())?;
            let options = crate::expansion::ExpanderOptions {
                pathname_expand: false,
                ..Default::default()
            };
            let mut words = vec![];
            for word in unexpanded_words {
                words.extend(
                    crate::expansion::full_expand_and_split_word_with_options(
                        shell, &params, word, &options,
                    )
                    .await?,
                );
            }
            candidates.extend(
                words
                    .into_iter()
                    .filter(|word| word.starts_with(context.token_to_complete)),
            );
        }

        if let Some(glob_pattern) = &self.glob_pattern {
            let pattern = patterns::Pattern::from(glob_pattern.as_str())
                .set_extended_globbing(shell.options().extended_globbing)
                .set_case_insensitive(shell.options().case_insensitive_pathname_expansion);

            let expansions = pattern
                .expand(
                    shell.working_dir(),
                    Some(&patterns::Pattern::accept_all_expand_filter),
                    &patterns::FilenameExpansionOptions::default(),
                )?
                .into_paths();

            candidates.extend(expansions);
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
        if let Some(filter_pattern) = &self.filter_pattern
            && !filter_pattern.is_empty()
        {
            let mut updated = Vec::new();

            // Like bash, a leading `!` (unless extglob is on and it starts a `!(...)`
            // pattern) inverts the filter, which then keeps what it matches.
            let (pattern, keep_matches) = match filter_pattern.strip_prefix('!') {
                Some(rest) if !(shell.options().extended_globbing && rest.starts_with('(')) => {
                    (rest, true)
                }
                _ => (filter_pattern.as_str(), false),
            };

            for candidate in candidates {
                let matches = completion_filter_pattern_matches(
                    pattern,
                    candidate.as_str(),
                    context.token_to_complete,
                    shell,
                )?;

                if matches == keep_matches {
                    updated.push(candidate);
                }
            }

            candidates = updated;
        }

        // Add prefix and/or suffix, if present.
        if self.prefix.is_some() || self.suffix.is_some() {
            let empty = String::new();
            let prefix = self.prefix.as_ref().unwrap_or(&empty);
            let suffix = self.suffix.as_ref().unwrap_or(&empty);

            let mut updated = Vec::with_capacity(candidates.len() * (prefix.len() + suffix.len()));
            for candidate in candidates {
                updated.push(std::format!("{prefix}{candidate}{suffix}"));
            }

            candidates = updated;
        }

        //
        // Now apply options
        //

        let options = if let Some(options) = &shell.completion_config().current_completion_options {
            options
        } else {
            &self.options
        };

        let mut processing_options = ProcessingOptions {
            treat_as_filenames: options.get(CompleteOption::FileNames),
            no_autoquote_filenames: options.get(CompleteOption::NoQuote),
            no_trailing_space_at_end_of_line: options.get(CompleteOption::NoSpace),
        };

        // plusdirs always adds directory names; dirnames only does so when nothing else matched.
        if options.get(CompleteOption::PlusDirs)
            || (options.get(CompleteOption::DirNames) && candidates.is_empty())
        {
            let mut dir_candidates = get_file_completions(
                shell,
                context.token_to_complete,
                /* must_be_dir */ true,
            )
            .await;

            // If directories are all we have, let them be marked as such.
            if candidates.is_empty() && shell.completion_config().fallback_options.mark_directories
            {
                processing_options.treat_as_filenames = true;
            }

            candidates.append(&mut dir_candidates);
        }

        // If we still have no candidates, and bashdefault completions were requested, then generate
        // those.
        if candidates.is_empty() && options.get(CompleteOption::BashDefault) {
            // TODO(completions): it's not clear what default "bash" completions means. From basic
            // testing, this doesn't seem to include basic file and directory name
            // completion.
            tracing::debug!(target: trace_categories::COMPLETION, "unimplemented: complete -o bashdefault");
        }

        // If we still have no candidates, and default completions were requested, then generate
        // those.
        if candidates.is_empty() && options.get(CompleteOption::Default) {
            // N.B. We approximate "default" readline completion behavior by getting file and
            // dir completions.
            let must_be_dir = options.get(CompleteOption::DirNames);

            let mut default_candidates =
                get_file_completions(shell, context.token_to_complete, must_be_dir).await;
            candidates.append(&mut default_candidates);

            if shell.completion_config().fallback_options.mark_directories {
                processing_options.treat_as_filenames = true;
            }
        }

        // Sort, unless blocked by options.
        if !self.options.get(CompleteOption::NoSort) {
            candidates.sort();
        }

        Ok(Answer::Candidates(candidates, processing_options))
    }

    #[expect(clippy::too_many_lines)]
    async fn generate_action_completions(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
        context: &Context<'_>,
    ) -> Result<Vec<String>, error::Error> {
        let mut candidates = Vec::new();

        let token = context.token_to_complete;

        for action in &self.actions {
            match action {
                CompleteAction::Alias => {
                    // Aliases are stored unordered; bash enumerates them sorted by name.
                    for name in shell.aliases().keys().sorted() {
                        if name.starts_with(token) {
                            candidates.push(name.clone());
                        }
                    }
                }
                CompleteAction::ArrayVar => {
                    for (name, var) in shell.env().iter() {
                        if var.value().is_array() && name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                }
                CompleteAction::Binding => {
                    for input_func in interfaces::InputFunction::iter() {
                        let name: &'static str = input_func.into();
                        if name.starts_with(token) {
                            candidates.push(name.to_string());
                        }
                    }
                }
                CompleteAction::Builtin => {
                    for name in shell.builtins().keys() {
                        if name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                }
                CompleteAction::Command => {
                    let command_completions =
                        get_external_command_completions(shell, context.token_to_complete);
                    candidates.extend(command_completions);
                    for name in shell.builtins().keys() {
                        if name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                    for keyword in shell.get_keywords() {
                        if keyword.starts_with(token) {
                            candidates.push(keyword.to_string());
                        }
                    }
                    // Functions are stored unordered; bash enumerates them sorted by name.
                    for (name, _) in shell.funcs().iter().sorted_by_key(|v| v.0) {
                        if name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                }
                CompleteAction::Directory => {
                    let mut file_completions =
                        get_file_completions(shell, context.token_to_complete, true).await;
                    candidates.append(&mut file_completions);
                }
                CompleteAction::Disabled => {
                    for (name, registration) in shell.builtins() {
                        if registration.disabled && name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                }
                CompleteAction::Enabled => {
                    for (name, registration) in shell.builtins() {
                        if !registration.disabled && name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                }
                CompleteAction::Export => {
                    for (key, value) in shell.env().iter() {
                        if value.is_exported() && key.starts_with(token) {
                            candidates.push(key.to_owned());
                        }
                    }
                }
                CompleteAction::File => {
                    let mut file_completions =
                        get_file_completions(shell, context.token_to_complete, false).await;
                    candidates.append(&mut file_completions);
                }
                CompleteAction::Function => {
                    // Functions are stored unordered; bash enumerates them sorted by name.
                    for (name, _) in shell.funcs().iter().sorted_by_key(|v| v.0) {
                        if name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                }
                CompleteAction::Group => {
                    for group_name in users::get_all_groups()? {
                        if group_name.starts_with(token) {
                            candidates.push(group_name);
                        }
                    }
                }
                CompleteAction::HelpTopic => {
                    // For now, we only have help topics for built-in commands.
                    for name in shell.builtins().keys() {
                        if name.starts_with(token) {
                            candidates.push(name.to_owned());
                        }
                    }
                }
                CompleteAction::HostName => {
                    // N.B. We only retrieve one hostname.
                    if let Ok(name) = sys::network::get_hostname() {
                        let name = name.to_string_lossy();
                        if name.starts_with(token) {
                            candidates.push(name.to_string());
                        }
                    }
                }
                CompleteAction::Job => {
                    for job in &shell.jobs().jobs {
                        let command_name = job.command_name();
                        if command_name.starts_with(token) {
                            candidates.push(command_name.to_owned());
                        }
                    }
                }
                CompleteAction::Keyword => {
                    for keyword in shell.get_keywords() {
                        if keyword.starts_with(token) {
                            candidates.push(keyword.to_string());
                        }
                    }
                }
                CompleteAction::Running => {
                    for job in &shell.jobs().jobs {
                        if matches!(job.state, jobs::JobState::Running) {
                            let command_name = job.command_name();
                            if command_name.starts_with(token) {
                                candidates.push(command_name.to_owned());
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
                            candidates.push(option.name.to_owned());
                        }
                    }
                }
                CompleteAction::ShOpt => {
                    for option in namedoptions::options(namedoptions::ShellOptionKind::Shopt).iter()
                    {
                        if option.name.starts_with(token) {
                            candidates.push(option.name.to_owned());
                        }
                    }
                }
                CompleteAction::Signal => {
                    for signal in traps::TrapSignal::iterator() {
                        if signal.as_str().starts_with(token) {
                            candidates.push(signal.as_str().to_string());
                        }
                    }
                }
                CompleteAction::Stopped => {
                    for job in &shell.jobs().jobs {
                        if matches!(job.state, jobs::JobState::Stopped) {
                            let command_name = job.command_name();
                            if command_name.starts_with(token) {
                                candidates.push(job.command_name().to_owned());
                            }
                        }
                    }
                }
                CompleteAction::User => {
                    for user_name in users::get_all_users()? {
                        if user_name.starts_with(token) {
                            candidates.push(user_name);
                        }
                    }
                }
                CompleteAction::Variable => {
                    for (key, _) in shell.env().iter() {
                        if key.starts_with(token) {
                            candidates.push(key.to_owned());
                        }
                    }
                }
            }
        }

        Ok(candidates)
    }

    async fn call_completion_command(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
        command_name: &str,
        context: &Context<'_>,
    ) -> Result<Vec<String>, error::Error> {
        // Move to a subshell so we can start filling out variables.
        let mut shell = shell.clone();

        let vars_and_values: [(&str, ShellValueLiteral); 4] = [
            ("COMP_LINE", context.input_line.into()),
            ("COMP_POINT", context.cursor_index.to_string().into()),
            ("COMP_KEY", context.trigger.comp_key().to_string().into()),
            ("COMP_TYPE", context.trigger.comp_type().to_string().into()),
        ];

        // Fill out variables.
        for (var, value) in vars_and_values {
            shell.env_mut().update_or_add(
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
        let candidates = output.lines().map(str::to_owned).collect();

        Ok(candidates)
    }

    async fn call_completion_function(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        function_name: &str,
        context: &Context<'_>,
    ) -> Result<Answer, error::Error> {
        // TODO(completions): Don't pollute the persistent environment with these?
        let vars_and_values: [(&str, ShellValueLiteral); 6] = [
            ("COMP_LINE", context.input_line.into()),
            ("COMP_POINT", context.cursor_index.to_string().into()),
            ("COMP_KEY", context.trigger.comp_key().to_string().into()),
            ("COMP_TYPE", context.trigger.comp_type().to_string().into()),
            (
                "COMP_WORDS",
                context
                    .tokens
                    .iter()
                    .map(|t| t.text)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            ("COMP_CWORD", context.token_index.to_string().into()),
        ];

        tracing::debug!(target: trace_categories::COMPLETION, "[calling completion func '{function_name}']: {}",
            vars_and_values.iter().map(|(k, v)| std::format!("{k}={v}")).collect::<Vec<String>>().join(" "));

        let mut vars_to_remove = Vec::with_capacity(vars_and_values.len());
        for (var, value) in vars_and_values {
            shell.env_mut().update_or_add(
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

        // Suppress trap delivery during completion function invocation.
        // N.B. We use manual acquire/release rather than an RAII guard because an
        // RAII guard would need to hold `&mut Shell`, preventing the mutable borrow
        // required by `invoke_function()`. This is safe because `invoke_result` is
        // captured into a variable (never early-returned with `?`), so
        // `release_trap_delivery_block()` always runs.
        shell.acquire_trap_delivery_block();

        let params = shell.default_exec_params();
        let invoke_result = shell
            .invoke_function(function_name, args.iter(), params)
            .await
            .map(|result| u8::from(result.exit_code));

        tracing::debug!(target: trace_categories::COMPLETION, "[completion function '{function_name}' returned: {invoke_result:?}]");

        shell.release_trap_delivery_block();

        // Make a best-effort attempt to unset the temporary variables.
        for var_name in vars_to_remove {
            let _ = shell.env_mut().unset(var_name);
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
            if let Some(reply) = shell.env_mut().unset("COMPREPLY")? {
                tracing::debug!(target: trace_categories::COMPLETION, "[completion function yielded: {reply:?}]");

                match reply.value() {
                    variables::ShellValue::IndexedArray(values) => {
                        return Ok(Answer::Candidates(
                            values.values().map(|v| v.to_owned()).collect(),
                            ProcessingOptions::default(),
                        ));
                    }
                    variables::ShellValue::String(s) => {
                        let candidates = vec![s.to_owned()];
                        return Ok(Answer::Candidates(candidates, ProcessingOptions::default()));
                    }
                    _ => (),
                }
            }

            Ok(Answer::Candidates(Vec::new(), ProcessingOptions::default()))
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
    pub candidates: Vec<String>,
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

/// Represents a token in the input line being completed.
#[derive(Debug, Clone, Copy)]
pub struct CompletionToken<'a> {
    /// The text of the token.
    pub text: &'a str,
    /// The start of the token, expressed as a byte offset into the input line.
    pub start: usize,
}

impl CompletionToken<'_> {
    /// Returns the length of the token, expressed as a byte count.
    pub const fn length(&self) -> usize {
        self.text.len()
    }

    /// Returns the end of the token, expressed as a byte offset into the input line.
    pub const fn end(&self) -> usize {
        self.start + self.length()
    }
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
    Candidates(Vec<String>, ProcessingOptions),
    /// The completion process needs to be restarted.
    RestartCompletionProcess,
}

impl Config {
    /// Removes all registered completion specs.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.specials.clear();
    }

    /// Ensures the named completion spec is no longer registered; returns whether a
    /// removal operation was required.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the completion spec to remove.
    pub fn remove(&mut self, name: SpecName<'_>) -> bool {
        match name {
            SpecName::Command(command) => self.commands.remove(command).is_some(),
            SpecName::Special(special) => self.specials.remove(&special).is_some(),
        }
    }

    /// Returns an iterator over the completion specs and their names, in no particular
    /// order.
    pub fn iter(&self) -> impl Iterator<Item = (SpecName<'_>, &Spec)> {
        let commands = self
            .commands
            .iter()
            .map(|(command, spec)| (SpecName::Command(command), spec));
        let specials = self
            .specials
            .iter()
            .map(|(special, spec)| (SpecName::Special(*special), spec));
        commands.chain(specials)
    }

    /// If present, returns the named completion spec.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the completion spec.
    pub fn get(&self, name: SpecName<'_>) -> Option<&Spec> {
        match name {
            SpecName::Command(command) => self.commands.get(command),
            SpecName::Special(special) => self.specials.get(&special),
        }
    }

    /// If present, returns a mutable reference to the named completion spec.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the completion spec.
    pub fn get_mut(&mut self, name: SpecName<'_>) -> Option<&mut Spec> {
        match name {
            SpecName::Command(command) => self.commands.get_mut(command),
            SpecName::Special(special) => self.specials.get_mut(&special),
        }
    }

    /// Registers the provided completion spec under the given name, replacing any
    /// already registered there.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the completion spec.
    /// * `spec` - The completion spec.
    pub fn set(&mut self, name: SpecName<'_>, spec: Spec) {
        match name {
            SpecName::Command(command) => {
                self.commands.insert(command.to_owned(), spec);
            }
            SpecName::Special(special) => {
                self.specials.insert(special, spec);
            }
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
        shell: &mut Shell<impl extensions::ShellExtensions>,
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
        let mut adjusted_tokens: Vec<&CompletionToken<'_>> = tokens.iter().collect();

        // Try to find which token we are in.
        for (i, token) in tokens.iter().enumerate() {
            // If the cursor is before the start of the token, then it's between
            // this token and the one that preceded it (or it's before the first
            // token if this is the first token).
            if cursor < token.start {
                // TODO(completions): Should insert an empty token here; the position looks to have
                // been between this token and the preceding one.
                completion_token_index = i;
                break;
            }
            // If the cursor is anywhere from the first char of the token up to
            // (and including) the first char after the token, then this we need
            // to generate completions to replace/update this token. We'll pay
            // attention to the position to figure out the prefix that we should
            // be completing.
            else if cursor >= token.start && cursor <= token.end() {
                // Update insertion index.
                insertion_index = token.start;

                // Update prefix.
                let offset_into_token = cursor - insertion_index;
                let token_str = token.text;
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
        let empty_token = CompletionToken {
            text: "",
            start: input.len(),
        };
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
                preceding_token: preceding_token.map(|t| t.text),
                command_name: adjusted_tokens.first().map(|token| token.text),
                input_line: input,
                token_index: completion_token_index,
                tokens: adjusted_tokens.as_slice(),
                cursor_index: position,
                trigger: CompletionTrigger::InteractiveComplete,
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
                candidates: Vec::new(),
                options: ProcessingOptions::default(),
            }),
        }
    }

    fn tokenize_input_for_completion<'a>(
        shell: &Shell<impl extensions::ShellExtensions>,
        input: &'a str,
    ) -> Vec<CompletionToken<'a>> {
        const FALLBACK: &str = " \t\n\"\'@><=;|&(:";

        let delimiter_str = shell
            .env_str("COMP_WORDBREAKS")
            .unwrap_or_else(|| FALLBACK.into());

        let delimiters: Vec<_> = delimiter_str.chars().collect();

        simple_tokenize_by_delimiters(input, delimiters.as_slice())
    }

    async fn get_completions_for_token(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        context: Context<'_>,
    ) -> Answer {
        // See if we can find a completion spec matching the current command.
        let mut found_spec: Option<&Spec> = None;

        if let Some(command_name) = context.command_name {
            if context.token_index == 0 {
                if let Some(spec) = self.specials.get(&SpecialSpec::InitialWord) {
                    found_spec = Some(spec);
                }
            } else {
                if let Some(spec) = shell.completion_config().commands.get(command_name) {
                    found_spec = Some(spec);
                } else if let Some(file_name) = PathBuf::from(command_name).file_name() {
                    if let Some(spec) = shell
                        .completion_config()
                        .commands
                        .get(&file_name.to_string_lossy().to_string())
                    {
                        found_spec = Some(spec);
                    }
                }

                if found_spec.is_none() {
                    if let Some(spec) = self.specials.get(&SpecialSpec::Default) {
                        found_spec = Some(spec);
                    }
                }
            }
        } else {
            if let Some(spec) = self.specials.get(&SpecialSpec::EmptyLine) {
                found_spec = Some(spec);
            }
        }

        // Try to generate completions.
        if let Some(spec) = found_spec {
            spec.to_owned()
                .get_completions(shell, &context)
                .await
                .unwrap_or_else(|_err| Answer::Candidates(Vec::new(), ProcessingOptions::default()))
        } else {
            // If we didn't find a spec, then fall back to basic completion.
            get_completions_using_basic_lookup(shell, &context).await
        }
    }
}

async fn get_file_completions(
    shell: &Shell<impl extensions::ShellExtensions>,
    token_to_complete: &str,
    must_be_dir: bool,
) -> Vec<String> {
    // Basic-expand the token-to-be-completed; it won't have been expanded to this point.
    let mut throwaway_shell = shell.clone();
    let params = throwaway_shell.default_exec_params();
    let options = expansion::ExpanderOptions {
        execute_command_substitutions: false,
        ..Default::default()
    };
    let expanded_token = expansion::basic_expand_word_with_options(
        &mut throwaway_shell,
        &params,
        &unquote_str(token_to_complete),
        &options,
    )
    .await
    .unwrap_or_else(|_err| token_to_complete.to_owned());

    // Normalize path separators before building the glob pattern, because backslash
    // is the escape character in glob syntax and must not be confused with a Windows
    // path separator.
    let expanded_token = sys::fs::normalize_path_separators(&expanded_token).into_owned();

    let glob = std::format!("{expanded_token}*");

    let path_filter = |path: &Path| !must_be_dir || shell.absolute_path(path).is_dir();

    let pattern = patterns::Pattern::from(glob)
        .set_extended_globbing(shell.options().extended_globbing)
        .set_case_insensitive(shell.options().case_insensitive_pathname_expansion);

    let mut completions: Vec<String> = pattern
        .expand(
            shell.working_dir(),
            Some(&path_filter),
            &patterns::FilenameExpansionOptions::default(),
        )
        .unwrap_or_default()
        .into_paths()
        .into_iter()
        .map(|p| match sys::fs::normalize_path_separators(&p) {
            std::borrow::Cow::Borrowed(_) => p,
            std::borrow::Cow::Owned(normalized) => normalized,
        })
        .collect();

    match expanded_token.as_str() {
        "." => {
            completions.push(".".into());
            completions.push("..".into());
        }
        ".." => {
            completions.push("..".into());
        }
        _ => {}
    }

    completions.sort();
    completions.dedup();
    completions
}

fn get_external_command_completions(
    shell: &Shell<impl extensions::ShellExtensions>,
    prefix: &str,
) -> impl Iterator<Item = String> {
    shell
        .find_executables_in_path_with_prefix(
            prefix,
            shell.options().case_insensitive_pathname_expansion,
        )
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
}

/// Attempts to complete a variable name from the given token.
/// Returns `Some(Answer)` if the token looks like a variable reference being typed,
/// or `None` if file/command completion should be used instead.
///
/// # Arguments
///
/// * `shell` - The shell instance to use for variable lookup.
/// * `token` - The token being completed. May be empty.
fn try_get_variable_completions(
    shell: &Shell<impl extensions::ShellExtensions>,
    token: &str,
) -> Option<Answer> {
    // Determine if this is a braced or unbraced variable reference
    let (var_prefix, use_braces) = if let Some(prefix) = token.strip_prefix("${") {
        // For braced: only complete if brace isn't closed yet
        if prefix.contains('}') {
            return None;
        }
        (prefix, true)
    } else {
        let prefix = token.strip_prefix('$')?;
        (prefix, false)
    };

    // If there's a path separator, this is a path like $HOME/foo, not a variable to complete
    if sys::fs::contains_path_separator(var_prefix) {
        return None;
    }

    // Find matching variables
    let mut candidates: Vec<String> = shell
        .env()
        .iter()
        .filter(|(key, _)| key.starts_with(var_prefix))
        .map(|(key, _)| {
            if use_braces {
                format!("${{{key}}}")
            } else {
                format!("${key}")
            }
        })
        .collect();
    candidates.sort();

    // Variable completions should not be treated as filenames (no escaping needed)
    let options = ProcessingOptions {
        treat_as_filenames: false,
        ..ProcessingOptions::default()
    };

    Some(Answer::Candidates(candidates, options))
}

/// Adds command-position completions to candidates.
/// This includes external commands, builtins, functions, aliases, and keywords.
fn add_command_completions(
    shell: &Shell<impl extensions::ShellExtensions>,
    prefix: &str,
    candidates: &mut Vec<String>,
) {
    // Add external commands.
    let command_completions = get_external_command_completions(shell, prefix);
    candidates.extend(command_completions);

    // Add built-in commands.
    for (name, registration) in shell.builtins() {
        if !registration.disabled && name.starts_with(prefix) {
            candidates.push(name.to_owned());
        }
    }

    // Add shell functions.
    for (name, _) in shell.funcs().iter() {
        if name.starts_with(prefix) {
            candidates.push(name.to_owned());
        }
    }

    // Add aliases.
    for name in shell.aliases().keys() {
        if name.starts_with(prefix) {
            candidates.push(name.to_owned());
        }
    }

    // Add keywords.
    for keyword in shell.get_keywords() {
        if keyword.starts_with(prefix) {
            candidates.push(keyword.to_string());
        }
    }
}

async fn get_completions_using_basic_lookup(
    shell: &Shell<impl extensions::ShellExtensions>,
    context: &Context<'_>,
) -> Answer {
    let token = context.token_to_complete;

    // Try variable completion first (e.g., $HO -> $HOME, ${HO -> ${HOME})
    if let Some(answer) = try_get_variable_completions(shell, token) {
        return answer;
    }

    // File completions
    let mut candidates = get_file_completions(shell, token, false).await;

    // If this appears to be the command token (and if there's *some* prefix without
    // a path separator) then also consider whether we should search the path for
    // completions too.
    // TODO(completions): Do a better job than just checking if index == 0.
    let is_command_position =
        context.token_index == 0 && !token.is_empty() && !sys::fs::contains_path_separator(token);

    if is_command_position {
        add_command_completions(shell, token, &mut candidates);
        candidates.sort();
    }

    Answer::Candidates(candidates, ProcessingOptions::default())
}

/// Tokenizes input by splitting on delimiter characters. Words (non-delimiter sequences)
/// are emitted as tokens. Consecutive non-whitespace delimiters are grouped into a single
/// token. Whitespace delimiters separate tokens but are not emitted themselves.
#[allow(clippy::string_slice, reason = "used indices come from char_indices")]
fn simple_tokenize_by_delimiters<'a>(
    input: &'a str,
    delimiters: &[char],
) -> Vec<CompletionToken<'a>> {
    let mut tokens = vec![];
    let mut word_start = None;
    let mut word_is_delimiters = false;
    let mut quote_char: Option<char> = None;
    let mut escaped = false;

    for (i, c) in input.char_indices() {
        let mut is_active_delimiter = false;
        if escaped {
            escaped = false;
        } else if let Some(q) = quote_char {
            if c == '\\' && q == '"' {
                // an escape in double-quoted string works as an escape.
                escaped = true;
            } else if c == q {
                // end of quote.
                quote_char = None;
            }
        } else {
            if c == '\\' {
                escaped = true;
            } else if word_start.is_none() && (c == '\'' || c == '\"') {
                // start a new quote.
                quote_char = Some(c);
            } else {
                is_active_delimiter = delimiters.contains(&c);
            }
        }

        if is_active_delimiter {
            // If we were building a regular word and this is a delimiter, then finish it.
            // Similarly, if this is a whitespace delimiter, finish any delimiter sequence.
            if let Some(start) = word_start {
                if !word_is_delimiters || c.is_ascii_whitespace() {
                    tokens.push(CompletionToken {
                        text: &input[start..i],
                        start,
                    });
                    word_start = None;
                    word_is_delimiters = false;
                }

                if !c.is_ascii_whitespace() {
                    if word_start.is_none() {
                        word_start = Some(i);
                        word_is_delimiters = true;
                    }
                }
            } else if !c.is_ascii_whitespace() {
                // Non-whitespace delimiter: start or continue delimiter sequence
                if word_start.is_none() {
                    word_start = Some(i);
                    word_is_delimiters = true;
                }
            }
        } else {
            // Regular character (not a delimiter). Finish any delimiter sequence.
            if word_is_delimiters {
                if let Some(start) = word_start {
                    tokens.push(CompletionToken {
                        text: &input[start..i],
                        start,
                    });
                    word_start = None;
                    word_is_delimiters = false;
                }
            }

            // Start or continue a word
            if word_start.is_none() {
                word_start = Some(i);
            }
        }
    }

    // Add any remaining delimiter sequence
    if let Some(start) = word_start {
        tokens.push(CompletionToken {
            text: &input[start..],
            start,
        });
    }

    tokens
}

fn completion_filter_pattern_matches(
    pattern: &str,
    candidate: &str,
    token_being_completed: &str,
    shell: &Shell<impl extensions::ShellExtensions>,
) -> Result<bool, error::Error> {
    let pattern = replace_unescaped_ampersands(pattern, token_being_completed);

    //
    // TODO(completions): Replace unescaped '&' with the word being completed.
    //

    let pattern = patterns::Pattern::from(pattern.as_ref())
        .set_extended_globbing(shell.options().extended_globbing)
        .set_case_insensitive(shell.options().case_insensitive_pathname_expansion);

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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_matches;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn completion_tokenization() {
        assert_matches!(
            simple_tokenize_by_delimiters("one two", &[' ']).as_slice(),
            [
                CompletionToken {
                    text: "one",
                    start: 0,
                },
                CompletionToken {
                    text: "two",
                    start: 4,
                }
            ]
        );

        assert_matches!(
            simple_tokenize_by_delimiters("one \t two", &[' ', '\t']).as_slice(),
            [
                CompletionToken {
                    text: "one",
                    start: 0,
                },
                CompletionToken {
                    text: "two",
                    start: 6,
                }
            ]
        );

        assert_matches!(simple_tokenize_by_delimiters("    ", &[' ']).as_slice(), []);

        assert_matches!(
            simple_tokenize_by_delimiters(":", &[':']).as_slice(),
            [CompletionToken {
                text: ":",
                start: 0,
            }]
        );

        assert_matches!(
            simple_tokenize_by_delimiters("a:::b", &[':', ' ']).as_slice(),
            [
                CompletionToken {
                    text: "a",
                    start: 0,
                },
                CompletionToken {
                    text: ":::",
                    start: 1,
                },
                CompletionToken {
                    text: "b",
                    start: 4,
                }
            ]
        );

        assert_matches!(
            simple_tokenize_by_delimiters("a: : :b", &[':', ' ']).as_slice(),
            [
                CompletionToken {
                    text: "a",
                    start: 0,
                },
                CompletionToken {
                    text: ":",
                    start: 1,
                },
                CompletionToken {
                    text: ":",
                    start: 3,
                },
                CompletionToken {
                    text: ":",
                    start: 5,
                },
                CompletionToken {
                    text: "b",
                    start: 6,
                }
            ]
        );

        assert_matches!(
            simple_tokenize_by_delimiters("one two:three", &[':', ' ']).as_slice(),
            [
                CompletionToken {
                    text: "one",
                    start: 0,
                },
                CompletionToken {
                    text: "two",
                    start: 4,
                },
                CompletionToken {
                    text: ":",
                    start: 7,
                },
                CompletionToken {
                    text: "three",
                    start: 8,
                }
            ]
        );

        assert_matches!(
            simple_tokenize_by_delimiters("one'two", &['\'']).as_slice(),
            [
                CompletionToken {
                    text: "one",
                    start: 0,
                },
                CompletionToken {
                    text: "'",
                    start: 3,
                },
                CompletionToken {
                    text: "two",
                    start: 4,
                },
            ]
        );

        assert_matches!(
            simple_tokenize_by_delimiters("one 'two:three'", &[':', ' ']).as_slice(),
            [
                CompletionToken {
                    text: "one",
                    start: 0,
                },
                CompletionToken {
                    text: "'two:three'",
                    start: 4,
                },
            ]
        );

        assert_matches!(
            simple_tokenize_by_delimiters("one \\'two \"two four\"", &[':', ' ']).as_slice(),
            [
                CompletionToken {
                    text: "one",
                    start: 0,
                },
                CompletionToken {
                    text: "\\'two",
                    start: 4,
                },
                CompletionToken {
                    text: "\"two four\"",
                    start: 10,
                },
            ]
        );
    }
}
