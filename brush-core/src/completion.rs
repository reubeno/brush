//! Implements programmable command completion support.
//!
//! There are two ways in:
//!
//! - [`Shell::complete`] completes a line at a cursor, as interactive completion does.
//! - [`Spec::generate`] generates a spec's candidates for a word on its own, as the
//!   `compgen` builtin does.
//!
//! Completing a line follows bash and readline:
//!
//! 1. The word to complete (the text candidates replace, and a completion function's
//!    `$2`) and the words of `COMP_WORDS` are found separately, and needn't agree: after
//!    `--opt=`, the word to complete is empty, while `COMP_WORDS` ends with `=`.
//! 2. The spec to use is looked up: `complete -E` if there's nothing before the cursor
//!    (and it isn't at the start of a word), `-I` for the initial word, else the command's,
//!    else `-D`. It's looked up again each time a completion function asks for completion
//!    to restart, so specs it registered are used.
//! 3. The spec generates candidates with a completion in progress on the shell, so
//!    `compopt` can change its options. With no spec, basic completion completes
//!    variables, file names, and commands.

//!
//! Each candidate comes back as an edit of the line: quoted as bash quotes it for readline,
//! and, as readline does, closing the word's quote, marking a directory, or adding a
//! trailing space, as fits. That's everything that decides what the line says; the line editor just
//! chooses which edit to make and how to show the candidates.

use itertools::Itertools;
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap},
    ops::Range,
    path::Path,
};
use strum::IntoEnumIterator;

use crate::{
    Shell, commands, env, error, escape, expansion, extensions, interfaces, jobs, namedoptions,
    patterns,
    sys::{self, users},
    trace_categories, traps,
    variables::{self, ShellValue, ShellVariable},
};
use brush_parser::unquote_str;

mod edits;
mod files;
mod quoting;
mod words;

use files::resolve_file_names;
use words::{LineWords, find_completion_word, find_line_words};

// `compgen -W` splits unquoted literal IFS characters before expanding each resulting word.
fn split_completion_word_list(
    word_list: &str,
    ifs: &str,
    parser_options: &brush_parser::ParserOptions,
) -> Result<Vec<String>, error::Error> {
    // Like bash, quoting left open at the end runs to the end of the list.
    let mut word_list = word_list.to_owned();
    quoting::close(&mut word_list);
    let word_list = word_list.as_str();

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

/// Encapsulates the shell's programmable command completion configuration.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    /// The specs for completing commands' arguments, by command name.
    commands: HashMap<String, Spec>,
    /// The specs used in place of a command's.
    specials: HashMap<SpecialSpec, Spec>,

    /// The line editor's preferences for how completions edit the line, as the `bind`
    /// builtin sets them.
    pub edit_prefs: EditPrefs,
}

/// A line editor's preferences for how completions edit the line: readline variables, such
/// as `mark-directories`.
///
/// The line editor passes them to [`Shell::complete`]. For now, the [`Config`] stores them,
/// since that's where the `bind` builtin can set them.
#[derive(Clone, Debug)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EditPrefs {
    /// If true, mark directory completions with a trailing slash.
    pub mark_directories: bool,
    /// If true, mark symlinked directory completions with a trailing slash.
    pub mark_symlinked_directories: bool,
}

impl Default for EditPrefs {
    fn default() -> Self {
        Self {
            mark_directories: true,
            mark_symlinked_directories: false,
        }
    }
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

/// A shell's programmable completion state.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct State {
    /// The completion specs and settings.
    pub(crate) config: Config,
    /// The programmable completion in progress, if any.
    pub(crate) in_progress: Option<InProgressCompletion>,
}

/// A programmable completion whose spec is generating candidates.
///
/// That includes while its completion function runs. Like bash, the `compopt` builtin
/// changes its options.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct InProgressCompletion {
    /// The options in effect, which start as the spec's.
    pub(crate) options: GenerationOptions,
}

/// Keeps a completion in progress on a shell until dropped, then restores the one (if any)
/// it replaced -- even if the completion is cancelled partway through, e.g. by Ctrl-C.
struct InProgressScope<'a, SE: extensions::ShellExtensions> {
    shell: &'a mut Shell<SE>,
    prev: Option<InProgressCompletion>,
}

impl<'a, SE: extensions::ShellExtensions> InProgressScope<'a, SE> {
    const fn start(shell: &'a mut Shell<SE>, in_progress: InProgressCompletion) -> Self {
        let prev = shell.in_progress_completion_mut().replace(in_progress);
        Self { shell, prev }
    }

    /// Ends the completion in progress early, returning its options as `compopt` left
    /// them, if it's still in progress.
    fn take_options(&mut self) -> Option<GenerationOptions> {
        self.shell
            .in_progress_completion_mut()
            .take()
            .map(|in_progress| in_progress.options)
    }
}

impl<SE: extensions::ShellExtensions> Drop for InProgressScope<'_, SE> {
    fn drop(&mut self) {
        *self.shell.in_progress_completion_mut() = self.prev.take();
    }
}

/// Enters a scope on `shell` holding the `COMP_*` variables `vars` for a completion function
/// or command (exported, if `export`), until the returned guard is dropped -- even if the
/// completion is cancelled.
fn enter_comp_vars_scope<SE: extensions::ShellExtensions>(
    shell: &mut Shell<SE>,
    vars: impl IntoIterator<Item = (&'static str, ShellValue)>,
    export: bool,
) -> Result<impl std::ops::DerefMut<Target = Shell<SE>> + '_, error::Error> {
    let mut scoped = shell.enter_scope(env::EnvironmentScope::Command);
    for (name, value) in vars {
        let mut var = ShellVariable::new(value);
        if export {
            var.export();
        }
        scoped
            .env_mut()
            .add(name, var, env::EnvironmentScope::Command)?;
    }
    Ok(scoped)
}

/// What a spec generates candidates for.
#[derive(Clone, Copy, Debug)]
struct Context<'a> {
    /// The word to generate candidates for. When completing a line, this is the word being
    /// completed; for [`Spec::generate`], it's the word given, dequoted.
    word: &'a str,
    /// The line a completion function or command is told about: the one being completed,
    /// or for [`Spec::generate`], like bash, an empty one being completed for `compgen`.
    line: LineContext<'a>,
}

/// A line being completed, which a completion function or command is told about in its
/// arguments and the `COMP_*` variables.
#[derive(Clone, Copy, Debug)]
struct LineContext<'a> {
    /// The line (`COMP_LINE`).
    input: &'a str,
    /// The cursor's byte offset in the line (`COMP_POINT`).
    cursor: usize,
    /// The words of `COMP_WORDS`, and which the cursor is in (`COMP_CWORD`).
    words: &'a LineWords<'a>,
    /// The name of the command being completed for (`$1`): the first word or, for the
    /// empty-line and initial-word specs, bash's names for them.
    command_name: Option<&'a str>,
    /// The kind of completion (`COMP_TYPE`), as bash's char code for it (e.g. Tab for
    /// normal completion), or 0 for `compgen`.
    comp_type: u8,
    /// The key that triggered completion (`COMP_KEY`), or 0 for `compgen`.
    comp_key: u8,
}

impl<'a> LineContext<'a> {
    /// Returns the line `input`, being completed at `cursor`, whose words are `words`.
    fn new(input: &'a str, cursor: usize, words: &'a LineWords<'a>) -> Self {
        Self {
            input,
            cursor,
            words,
            command_name: words.command_name(),
            comp_type: b'\t',
            comp_key: b'\t',
        }
    }

    /// Returns the arguments a completion function or command gets for completing `word`:
    /// like bash, the command name, the word, and the word before it, each empty if
    /// there's none.
    fn args(&self, word: &'a str) -> [&'a str; 3] {
        [
            self.command_name.unwrap_or(""),
            word,
            self.words.preceding_word().unwrap_or(""),
        ]
    }

    /// Returns the `COMP_*` variables that describe the line to a completion function or
    /// command, except for `COMP_WORDS` and `COMP_CWORD` (see [`Self::comp_words_vars`]).
    fn comp_vars(&self) -> Vec<(&'static str, ShellValue)> {
        vec![
            ("COMP_LINE", self.input.into()),
            ("COMP_POINT", self.cursor.to_string().into()),
            ("COMP_KEY", self.comp_key.to_string().into()),
            ("COMP_TYPE", self.comp_type.to_string().into()),
        ]
    }

    /// Returns the `COMP_WORDS` and `COMP_CWORD` variables, which only completion
    /// functions get.
    fn comp_words_vars(&self) -> [(&'static str, ShellValue); 2] {
        let cword = self
            .words
            .cword
            .map_or_else(|| "-1".to_owned(), |i| i.to_string());
        [
            ("COMP_WORDS", self.words.comp_words.clone().into()),
            ("COMP_CWORD", cword.into()),
        ]
    }
}

/// Candidates a spec generated (`T`), or else a completion function's request, by
/// returning 124, that completion restart.
enum Generated<T> {
    /// The candidates generated.
    Candidates(T),
    /// A request that completion restart.
    Restart,
}

/// The candidates a spec's generators produced.
struct SpecCandidates {
    /// The candidates.
    candidates: Vec<String>,
    /// Whether generating them makes them file names: like bash, if the spec's `file` action
    /// ran, or its `directory` action found any (see [`CompleteOption::FileNames`]).
    file_names: bool,
}

impl Spec {
    /// Generates this spec's completion candidates for `word`, a word on its own (not
    /// part of a line being completed), as the `compgen` builtin does: with any fallbacks
    /// its options ask for added, and like bash, in the order generated (sorting is for
    /// showing them to a user). A completion function or command run for `word` is told,
    /// like bash's, about an empty line being completed for `compgen`.
    ///
    /// This doesn't start a completion in progress, so the `compopt` builtin can't change
    /// the options applied to the candidates. Like bash, a completion function that asks
    /// for completion to restart generates no candidates.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance to use for completion generation.
    /// * `word` - The word to generate candidates for, as typed (with any quoting).
    pub async fn generate(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        word: &str,
    ) -> Result<Vec<String>, crate::error::Error> {
        let word = unquote_str(word);
        let context = Context {
            word: &word,
            line: LineContext {
                input: "",
                cursor: 0,
                words: &words::NO_WORDS,
                command_name: Some("compgen"),
                comp_type: 0,
                comp_key: 0,
            },
        };

        let Generated::Candidates(SpecCandidates { candidates, .. }) =
            self.generate_candidates(shell, &context).await?
        else {
            return Ok(Vec::new());
        };

        let (candidates, _options) = self
            .apply_options(shell, &context, candidates, self.options.clone())
            .await;
        Ok(candidates)
    }

    /// Completes the word being completed in `context`'s line, with a completion in
    /// progress for it (see [`InProgressCompletion`]) while its candidates are generated, so
    /// the `compopt` builtin can change the options applied to them. Returns the candidates,
    /// sorted unless `nosort`, and the options applied to them.
    async fn complete(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        context: &Context<'_>,
    ) -> Result<Generated<(Vec<String>, GenerationOptions)>, crate::error::Error> {
        let mut scope = InProgressScope::start(
            shell,
            InProgressCompletion {
                options: self.options.clone(),
            },
        );

        let Generated::Candidates(SpecCandidates {
            candidates,
            file_names,
        }) = self.generate_candidates(scope.shell, context).await?
        else {
            return Ok(Generated::Restart);
        };

        // Apply the options as `compopt` left them, and as generating the candidates did.
        let mut options = scope.take_options().unwrap_or_else(|| self.options.clone());
        if file_names {
            options.set(CompleteOption::FileNames, true);
        }
        let sort = !options.get(CompleteOption::NoSort);
        let (mut candidates, options) = self
            .apply_options(scope.shell, context, candidates, options)
            .await;
        if sort {
            candidates.sort();
        }
        Ok(Generated::Candidates((candidates, options)))
    }

    /// Generates this spec's candidates, before its options are applied.
    async fn generate_candidates(
        &self,
        shell: &mut Shell<impl extensions::ShellExtensions>,
        context: &Context<'_>,
    ) -> Result<Generated<SpecCandidates>, crate::error::Error> {
        // Generate completions based on any provided actions (and on words).
        let SpecCandidates {
            mut candidates,
            file_names,
        } = self.generate_action_completions(shell, context).await?;
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
                    .filter(|word| word.starts_with(context.word)),
            );
        }

        if let Some(glob_pattern) = &self.glob_pattern {
            let pattern = shell_pattern(shell, glob_pattern.as_str());

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
            match call_completion_function(shell, function_name, context).await? {
                Generated::Candidates(new_candidates) => candidates.extend(new_candidates),
                Generated::Restart => return Ok(Generated::Restart),
            }
        }
        if let Some(command) = &self.command {
            candidates.extend(call_completion_command(shell, command, context).await?);
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
                    context.word,
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

        Ok(Generated::Candidates(SpecCandidates {
            candidates,
            file_names,
        }))
    }

    /// Applies `options` to the candidates this spec generated, adding any fallbacks they
    /// ask for; returns them and the options in effect for them, which have `filenames`
    /// enabled if a fallback added only file names.
    async fn apply_options(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
        context: &Context<'_>,
        mut candidates: Vec<String>,
        mut options: GenerationOptions,
    ) -> (Vec<String>, GenerationOptions) {
        // plusdirs always adds directory names; dirnames only does so when nothing else matched.
        if options.get(CompleteOption::PlusDirs)
            || (options.get(CompleteOption::DirNames) && candidates.is_empty())
        {
            let mut dir_candidates =
                get_file_completions(shell, context.word, /* must_be_dir */ true).await;

            // If directories are all we have, they're file names.
            if candidates.is_empty() {
                options.set(CompleteOption::FileNames, true);
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
                get_file_completions(shell, context.word, must_be_dir).await;
            candidates.append(&mut default_candidates);

            options.set(CompleteOption::FileNames, true);
        }

        (candidates, options)
    }

    /// Generates the candidates of this spec's actions.
    #[expect(clippy::too_many_lines, reason = "a flat match over the actions")]
    async fn generate_action_completions(
        &self,
        shell: &Shell<impl extensions::ShellExtensions>,
        context: &Context<'_>,
    ) -> Result<SpecCandidates, error::Error> {
        let prefix = context.word;
        let mut candidates = Vec::new();
        let mut file_names = false;

        for action in &self.actions {
            let c = &mut candidates;
            match action {
                // Aliases and functions are stored unordered; bash enumerates them sorted
                // by name.
                CompleteAction::Alias => {
                    extend_matching(c, shell.aliases().keys().sorted(), prefix);
                }
                CompleteAction::ArrayVar => extend_matching(
                    c,
                    shell
                        .env()
                        .iter()
                        .filter(|(_, var)| var.value().is_array())
                        .map(|(name, _)| name),
                    prefix,
                ),
                CompleteAction::Binding => extend_matching(
                    c,
                    interfaces::InputFunction::iter().map(<&'static str>::from),
                    prefix,
                ),
                // For now, we only have help topics for built-in commands.
                CompleteAction::Builtin | CompleteAction::HelpTopic => {
                    extend_matching(c, shell.builtins().keys(), prefix);
                }
                CompleteAction::Command => c.extend(command_completions(shell, prefix)),
                CompleteAction::Directory => {
                    let dirs = get_file_completions(shell, context.word, true).await;
                    file_names |= !dirs.is_empty();
                    c.extend(dirs);
                }
                CompleteAction::Disabled => extend_matching(
                    c,
                    shell
                        .builtins()
                        .iter()
                        .filter(|(_, registration)| registration.disabled)
                        .map(|(name, _)| name),
                    prefix,
                ),
                CompleteAction::Enabled => extend_matching(
                    c,
                    shell
                        .builtins()
                        .iter()
                        .filter(|(_, registration)| !registration.disabled)
                        .map(|(name, _)| name),
                    prefix,
                ),
                CompleteAction::Export => extend_matching(
                    c,
                    shell
                        .env()
                        .iter()
                        .filter(|(_, var)| var.is_exported())
                        .map(|(name, _)| name),
                    prefix,
                ),
                CompleteAction::File => {
                    file_names = true;
                    c.extend(get_file_completions(shell, context.word, false).await);
                }
                CompleteAction::Function => extend_matching(
                    c,
                    shell.funcs().iter().map(|(name, _)| name).sorted(),
                    prefix,
                ),
                CompleteAction::Group => extend_matching(c, users::get_all_groups()?, prefix),
                // N.B. We only retrieve one hostname.
                CompleteAction::HostName => extend_matching(
                    c,
                    sys::network::get_hostname()
                        .ok()
                        .map(|name| name.to_string_lossy().into_owned()),
                    prefix,
                ),
                CompleteAction::Job => extend_matching(
                    c,
                    shell.jobs().jobs.iter().map(|job| job.command_name()),
                    prefix,
                ),
                CompleteAction::Keyword => extend_matching(c, shell.get_keywords(), prefix),
                CompleteAction::Running => extend_matching(
                    c,
                    shell
                        .jobs()
                        .jobs
                        .iter()
                        .filter(|job| matches!(job.state, jobs::JobState::Running))
                        .map(|job| job.command_name()),
                    prefix,
                ),
                CompleteAction::Service => {
                    tracing::debug!(target: trace_categories::COMPLETION, "unimplemented: complete -A service");
                }
                CompleteAction::SetOpt => extend_matching(
                    c,
                    namedoptions::options(namedoptions::ShellOptionKind::SetO)
                        .iter()
                        .map(|option| option.name),
                    prefix,
                ),
                CompleteAction::ShOpt => extend_matching(
                    c,
                    namedoptions::options(namedoptions::ShellOptionKind::Shopt)
                        .iter()
                        .map(|option| option.name),
                    prefix,
                ),
                CompleteAction::Signal => extend_matching(
                    c,
                    traps::TrapSignal::iterator().map(traps::TrapSignal::as_str),
                    prefix,
                ),
                CompleteAction::Stopped => extend_matching(
                    c,
                    shell
                        .jobs()
                        .jobs
                        .iter()
                        .filter(|job| matches!(job.state, jobs::JobState::Stopped))
                        .map(|job| job.command_name()),
                    prefix,
                ),
                CompleteAction::User => extend_matching(c, users::get_all_users()?, prefix),
                CompleteAction::Variable => {
                    extend_matching(c, shell.env().iter().map(|(name, _)| name), prefix);
                }
            }
        }

        Ok(SpecCandidates {
            candidates,
            file_names,
        })
    }
}

/// Runs the completion command `command` (`complete -C`) to generate candidates for the
/// word `context` is generating them for: one per line of its output.
async fn call_completion_command(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    command: &str,
    context: &Context<'_>,
) -> Result<Vec<String>, error::Error> {
    let line = context.line;

    // Compose the full command line.
    let mut command_line = command.to_owned();
    for arg in line.args(context.word) {
        command_line.push(' ');
        command_line.push_str(&escape::quote_if_needed(
            arg,
            escape::QuoteMode::SingleQuote,
        ));
    }

    // Run the command in a subshell, with the variables exported to it and traps blocked.
    // Like a command substitution, it would set `$?`; the user didn't run it, so leave that
    // as it was.
    let status = shell.save_command_status();
    let mut scoped = enter_comp_vars_scope(shell, line.comp_vars(), true)?;
    let mut blocked = scoped.block_trap_delivery();
    let params = blocked.default_exec_params();
    let output =
        commands::invoke_command_in_subshell_and_get_output(&mut blocked, &params, command_line)
            .await;
    drop(blocked);
    drop(scoped);
    shell.restore_command_status(status);

    Ok(output?.lines().map(str::to_owned).collect())
}

/// Runs the completion function `function_name` (`complete -F`) to generate candidates for
/// the word `context` is generating them for: those it leaves in `COMPREPLY`, unless it
/// asks for completion to restart.
async fn call_completion_function(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    function_name: &str,
    context: &Context<'_>,
) -> Result<Generated<Vec<String>>, error::Error> {
    let line = context.line;
    let mut vars = line.comp_vars();
    vars.extend(line.comp_words_vars());

    tracing::debug!(target: trace_categories::COMPLETION, "[calling completion func '{function_name}']: {}",
        vars.iter().map(|(k, v)| std::format!("{k}={v:?}")).collect::<Vec<String>>().join(" "));

    // Set the variables in a scope of their own, left when the function returns -- or when
    // the completion is cancelled -- and block traps while it runs.
    let names: Vec<_> = vars.iter().map(|(name, _)| *name).collect();
    let mut scoped = enter_comp_vars_scope(shell, vars, false)?;
    let mut blocked = scoped.block_trap_delivery();
    let params = blocked.default_exec_params();
    let invoke_result = blocked
        .invoke_function(function_name, line.args(context.word).iter(), params)
        .await
        .map(|result| u8::from(result.exit_code));

    // The function's done: unblock traps, and unset the variables.
    drop(blocked);
    drop(scoped);

    // Like bash, also unset any the function set again after unsetting them, or that were
    // set before it ran: either way, they're outside the scope just left.
    for name in names {
        let _ = shell.env_mut().unset(name);
    }

    tracing::debug!(target: trace_categories::COMPLETION, "[completion function '{function_name}' returned: {invoke_result:?}]");

    let result = invoke_result.unwrap_or_else(|e| {
        tracing::warn!(target: trace_categories::COMPLETION, "error while running completion function '{function_name}': {e}");
        1 // Report back a non-zero exit code.
    });

    // Like bash, take `COMPREPLY` out of the shell even if it's discarded below.
    let reply = shell.env_mut().unset("COMPREPLY")?;

    // When the function returns the special value 124, then it's a request
    // for us to restart the completion process.
    if result == 124 {
        return Ok(Generated::Restart);
    }

    tracing::debug!(target: trace_categories::COMPLETION, "[completion function yielded: {reply:?}]");

    let candidates = match reply.as_ref().map(|reply| reply.value()) {
        Some(variables::ShellValue::IndexedArray(values)) => {
            values.values().map(|v| v.to_owned()).collect()
        }
        Some(variables::ShellValue::String(s)) => vec![s.to_owned()],
        _ => Vec::new(),
    };
    Ok(Generated::Candidates(candidates))
}

/// The completions of the word at the cursor in a line, as edits of the line (see
/// [`Shell::complete`]).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Completions {
    /// The candidates, without duplicates, in the order generated: sorted, unless the spec
    /// said not to ([`CompleteOption::NoSort`]).
    pub candidates: Vec<Candidate>,
}

/// A completion candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Candidate {
    /// The candidate, unquoted, as generated: e.g. a file name, with its directory as typed
    /// (such as `~/Documents`).
    pub value: String,
    /// What kind of candidate it is.
    pub kind: CandidateKind,
    /// The edit of the line that completes the word with the candidate.
    pub edit: Edit,
}

/// What kind of candidate a [`Candidate`] is.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CandidateKind {
    /// A file name: a candidate completed as one, which applies to all of a completion's
    /// candidates -- if [`CompleteOption::FileNames`] is on, or like bash, the spec's `file`
    /// action ran or its `directory` action found any. That includes the command names that
    /// basic completion offers along with file names.
    FileName {
        /// Whether it names a directory.
        is_dir: bool,
    },
    /// Anything else.
    Other,
}

/// An edit of a line: replacing a range of it with text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Edit {
    /// The byte range of the line to replace. For a candidate, it's the word being completed
    /// (starting at the quote it's in, if any), up to the cursor -- or just past it, if the
    /// edit closes the word's quote in place of a closing quote there.
    pub replace: Range<usize>,
    /// The text to put in its place, quoted as needed; the cursor goes after it.
    pub text: String,
}

/// A candidate, once it's known what it names, before it's made an edit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ResolvedCandidate {
    /// The candidate's text, unquoted.
    text: String,
    /// What kind of candidate it is.
    kind: CandidateKind,
}

impl ResolvedCandidate {
    /// Returns a candidate with the given text, which isn't a file name.
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: CandidateKind::Other,
        }
    }

    /// Returns a candidate for the file name `text`, which, as when resolving file names,
    /// names a directory if it ends with a `/`.
    #[cfg(test)]
    fn file_name(text: &str) -> Self {
        Self {
            kind: CandidateKind::FileName {
                is_dir: text.ends_with('/'),
            },
            ..Self::new(text)
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
    /// the `complete` and `compopt` builtins accept it in place of a command name, and a
    /// completion function for the empty-line or initial-word spec gets it as `$1`.
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

    /// Returns the completion spec to use for completing `line`, and the name of the command
    /// it completes for (a completion function's `$1`): the empty-line spec (`complete -E`)
    /// if there's nothing before the cursor and it isn't at the start of a word, else the
    /// spec for the initial word (`complete -I`) if completing that, else its command's spec
    /// (by name, or by file name if the command is a path), else the default spec
    /// (`complete -D`). Like bash, the empty-line and initial-word specs complete for
    /// commands named after them.
    fn find_spec<'l>(&self, line: &LineContext<'l>) -> Option<(&Spec, Option<&'l str>)> {
        let special = |special: SpecialSpec| {
            let spec = self.specials.get(&special)?;
            Some((spec, Some(special.command_name())))
        };

        // Like bash, it's the empty line only if the cursor is at its very start, and not at
        // the start of a word there: with whitespace before the cursor, or a word just after
        // it, the initial word is completed.
        if line.cursor == 0 && line.input.chars().next().is_none_or(char::is_whitespace) {
            return special(SpecialSpec::EmptyLine);
        }

        if matches!(line.words.cword, None | Some(0)) {
            return special(SpecialSpec::InitialWord);
        }

        let command_name = line.command_name?;
        let spec = self
            .commands
            .get(command_name)
            .or_else(|| {
                let file_name = Path::new(command_name).file_name()?;
                self.commands.get(file_name.to_string_lossy().as_ref())
            })
            .or_else(|| self.specials.get(&SpecialSpec::Default))?;
        Some((spec, Some(command_name)))
    }
}

/// Completes `input` at `cursor`: see [`Shell::complete`].
pub(crate) async fn complete(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    input: &str,
    cursor: usize,
    prefs: &EditPrefs,
) -> Result<Completions, error::Error> {
    /// How many times a completion function may ask for completion to restart.
    const MAX_RESTARTS: u32 = 10;

    if !input.is_char_boundary(cursor) {
        return Err(error::ErrorKind::InvalidCompletionPosition(cursor).into());
    }

    let word_breaks = word_break_chars(shell);
    let word = find_completion_word(input, &word_breaks, cursor);
    let line_words = find_line_words(input, &word_breaks, cursor);
    let context = Context {
        word: &word.text,
        line: LineContext::new(input, cursor, &line_words),
    };

    // Complete, restarting as often as completion functions ask to. If they never stop
    // asking, there's nothing to complete with.
    let mut completed = None;
    for _ in 0..=MAX_RESTARTS {
        if let Generated::Candidates(candidates_and_options) = complete_word(shell, context).await {
            completed = Some(candidates_and_options);
            break;
        }
    }
    let (candidates, options) = completed.unwrap_or_else(|| {
        tracing::warn!(target: trace_categories::COMPLETION, "completion kept restarting; giving up");
        (Vec::new(), GenerationOptions::default())
    });

    let candidates = if options.get(CompleteOption::FileNames) {
        resolve_file_names(shell, candidates)
    } else {
        candidates.into_iter().map(ResolvedCandidate::new).collect()
    };

    let edits = edits::CandidateEdits {
        line: input,
        word: word.range.clone(),
        open_quote: word.quoting.quote,
        options: &options,
        prefs,
    };
    Ok(edits.completions(candidates))
}

fn word_break_chars(shell: &Shell<impl extensions::ShellExtensions>) -> Vec<char> {
    const FALLBACK: &str = " \t\n\"\'@><=;|&(:";

    shell
        .env_str("COMP_WORDBREAKS")
        .unwrap_or_else(|| FALLBACK.into())
        .chars()
        .collect()
}

/// Completes the word being completed in `context`'s line, with the completion spec that
/// applies to it if there is one, or else with basic completion. Returns the candidates and
/// the options in effect for them.
async fn complete_word(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    mut context: Context<'_>,
) -> Generated<(Vec<String>, GenerationOptions)> {
    // Look the spec up afresh each time: a completion function that asks for completion to
    // restart may have registered a new one.
    let Some((spec, command_name)) = shell.completion_config().find_spec(&context.line) else {
        return Generated::Candidates(get_completions_using_basic_lookup(shell, &context).await);
    };
    let spec = spec.clone();
    context.line.command_name = command_name;

    spec.complete(shell, &context).await.unwrap_or_else(|err| {
        tracing::debug!(target: trace_categories::COMPLETION, "completion spec failed: {err}");
        Generated::Candidates((Vec::new(), GenerationOptions::default()))
    })
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

    let pattern = shell_pattern(shell, glob);

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

/// Attempts to complete a variable name from the given token.
/// Returns the candidates if the token looks like a variable reference being typed,
/// or `None` if file/command completion should be used instead.
///
/// # Arguments
///
/// * `shell` - The shell instance to use for variable lookup.
/// * `token` - The token being completed. May be empty.
fn try_get_variable_completions(
    shell: &Shell<impl extensions::ShellExtensions>,
    token: &str,
) -> Option<Vec<String>> {
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

    Some(candidates)
}

/// Returns the names of the commands that start with `prefix`, in the order bash lists
/// them: aliases, keywords, functions, enabled builtins, then executables in the path.
fn command_completions(
    shell: &Shell<impl extensions::ShellExtensions>,
    prefix: &str,
) -> Vec<String> {
    let mut names = Vec::new();

    // Aliases, functions, and builtins are stored unordered; bash enumerates each sorted by
    // name.
    extend_matching(&mut names, shell.aliases().keys().sorted(), prefix);
    extend_matching(&mut names, shell.get_keywords(), prefix);
    extend_matching(
        &mut names,
        shell.funcs().iter().map(|(name, _)| name).sorted(),
        prefix,
    );
    extend_matching(
        &mut names,
        shell
            .builtins()
            .iter()
            .filter(|(_, registration)| !registration.disabled)
            .map(|(name, _)| name)
            .sorted(),
        prefix,
    );
    names.extend(
        shell
            .find_executables_in_path_with_prefix(
                prefix,
                shell.options().case_insensitive_pathname_expansion,
            )
            .filter_map(|path| Some(path.file_name()?.to_string_lossy().into_owned())),
    );

    names
}

/// Adds the `names` that start with `prefix` to `candidates`.
fn extend_matching<S: AsRef<str>>(
    candidates: &mut Vec<String>,
    names: impl IntoIterator<Item = S>,
    prefix: &str,
) {
    candidates.extend(
        names
            .into_iter()
            .filter(|name| name.as_ref().starts_with(prefix))
            .map(|name| name.as_ref().to_owned()),
    );
}

async fn get_completions_using_basic_lookup(
    shell: &Shell<impl extensions::ShellExtensions>,
    context: &Context<'_>,
) -> (Vec<String>, GenerationOptions) {
    let token = context.word;

    // Try variable completion first (e.g., $HO -> $HOME, ${HO -> ${HOME})
    // (Variable names aren't file names, so aren't quoted as such.)
    if let Some(candidates) = try_get_variable_completions(shell, token) {
        return (candidates, GenerationOptions::default());
    }

    // File completions
    let mut candidates = get_file_completions(shell, token, false).await;

    // If this appears to be the command token (and if there's *some* prefix without
    // a path separator) then also consider whether we should search the path for
    // completions too.
    // TODO(completions): Do a better job than just checking if index == 0.
    let is_command_position = context.line.words.cword == Some(0)
        && !token.is_empty()
        && !sys::fs::contains_path_separator(token);

    if is_command_position {
        candidates.extend(command_completions(shell, token));
        candidates.sort();
    }

    (
        candidates,
        std::iter::once(CompleteOption::FileNames).collect(),
    )
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

    let pattern = shell_pattern(shell, pattern.as_ref());

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

/// Returns `pattern`, set to match as the shell's options say.
fn shell_pattern(
    shell: &Shell<impl extensions::ShellExtensions>,
    pattern: impl Into<patterns::Pattern>,
) -> patterns::Pattern {
    pattern
        .into()
        .set_extended_globbing(shell.options().extended_globbing)
        .set_case_insensitive(shell.options().case_insensitive_pathname_expansion)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_options_enable_and_disable_each_option() {
        let mut options: GenerationOptions = std::iter::once(CompleteOption::NoSpace).collect();
        assert!(options.get(CompleteOption::NoSpace));
        assert!(!options.get(CompleteOption::NoSort));

        options.set(CompleteOption::NoSort, true);
        options.set(CompleteOption::NoSpace, false);
        assert!(options.get(CompleteOption::NoSort));
        assert!(!options.get(CompleteOption::NoSpace));
        assert_eq!(
            options,
            std::iter::once(CompleteOption::NoSort).collect::<GenerationOptions>()
        );
    }

    #[test]
    fn special_specs_have_bash_names() {
        assert_eq!(SpecialSpec::Default.command_name(), "_DefaultCmD_");
        assert_eq!(SpecialSpec::EmptyLine.command_name(), "_EmptycmD_");
        assert_eq!(SpecialSpec::InitialWord.command_name(), "_InitialWorD_");
    }

    #[test]
    fn special_specs_are_each_stored_on_their_own() {
        let mut config = Config::default();
        let specials = [
            (SpecialSpec::Default, "d"),
            (SpecialSpec::EmptyLine, "e"),
            (SpecialSpec::InitialWord, "i"),
        ];

        for (special, word) in specials {
            assert!(config.get(SpecName::Special(special)).is_none());
            let spec = Spec {
                word_list: Some(word.to_owned()),
                ..Spec::default()
            };
            config.set(SpecName::Special(special), spec);
        }

        for (special, word) in specials {
            let spec = config.get(SpecName::Special(special));
            assert_eq!(spec.and_then(|spec| spec.word_list.as_deref()), Some(word));

            if let Some(spec) = config.get_mut(SpecName::Special(special)) {
                spec.prefix = Some(word.to_uppercase());
            }
            // Bash's name for the spec names the same one.
            let spec = config.get(SpecName::parse(special.command_name()));
            assert_eq!(
                spec.and_then(|spec| spec.prefix.clone()),
                Some(word.to_uppercase())
            );
        }
    }

    #[test]
    fn spec_names_parse_bash_names_for_special_specs() {
        assert_eq!(
            SpecName::parse("_DefaultCmD_"),
            SpecName::Special(SpecialSpec::Default)
        );
        assert_eq!(SpecName::parse("mycmd"), SpecName::Command("mycmd"));
        for special in SpecialSpec::iter() {
            let name = SpecName::Special(special);
            assert_eq!(SpecName::parse(name.as_str()), name);
        }
        assert_eq!(SpecName::Command("mycmd").as_str(), "mycmd");
    }

    #[test]
    fn special_specs_round_trip_through_bash_names() {
        for special in SpecialSpec::iter() {
            assert_eq!(
                SpecialSpec::from_command_name(special.command_name()),
                Some(special)
            );
        }
        assert_eq!(
            SpecialSpec::from_command_name("_DefaultCmD_"),
            Some(SpecialSpec::Default)
        );
        assert_eq!(SpecialSpec::from_command_name("mycmd"), None);
    }

    #[test]
    fn completion_names_are_bash_names() {
        for action in CompleteAction::iter() {
            assert_eq!(<&str>::from(action).parse::<CompleteAction>(), Ok(action));
        }
        for option in CompleteOption::iter() {
            assert_eq!(<&str>::from(&option).parse::<CompleteOption>(), Ok(option));
        }
        assert_eq!(<&str>::from(CompleteAction::ArrayVar), "arrayvar");
        assert_eq!(<&str>::from(CompleteOption::BashDefault), "bashdefault");
    }
}
