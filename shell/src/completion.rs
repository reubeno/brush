use clap::ValueEnum;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{env, error, patterns, variables::ShellValueLiteral, Shell};

#[derive(Clone, Debug, ValueEnum)]
pub enum CompleteAction {
    #[clap(name = "alias")]
    Alias,
    #[clap(name = "arrayvar")]
    ArrayVar,
    #[clap(name = "binding")]
    Binding,
    #[clap(name = "builtin")]
    Builtin,
    #[clap(name = "command")]
    Command,
    #[clap(name = "directory")]
    Directory,
    #[clap(name = "disabled")]
    Disabled,
    #[clap(name = "enabled")]
    Enabled,
    #[clap(name = "export")]
    Export,
    #[clap(name = "file")]
    File,
    #[clap(name = "function")]
    Function,
    #[clap(name = "group")]
    Group,
    #[clap(name = "helptopic")]
    HelpTopic,
    #[clap(name = "hostname")]
    HostName,
    #[clap(name = "job")]
    Job,
    #[clap(name = "keyword")]
    Keyword,
    #[clap(name = "running")]
    Running,
    #[clap(name = "service")]
    Service,
    #[clap(name = "setopt")]
    SetOpt,
    #[clap(name = "shopt")]
    ShOpt,
    #[clap(name = "signal")]
    Signal,
    #[clap(name = "stopped")]
    Stopped,
    #[clap(name = "user")]
    User,
    #[clap(name = "variable")]
    Variable,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, ValueEnum)]
pub enum CompleteOption {
    #[clap(name = "bashdefault")]
    BashDefault,
    #[clap(name = "default")]
    Default,
    #[clap(name = "dirnames")]
    DirNames,
    #[clap(name = "filenames")]
    FileNames,
    #[clap(name = "noquote")]
    NoQuote,
    #[clap(name = "nosort")]
    NoSort,
    #[clap(name = "nospace")]
    NoSpace,
    #[clap(name = "plusdirs")]
    PlusDirs,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Default)]
pub struct CompletionConfig {
    pub commands: HashMap<String, CompletionSpec>,

    pub default: Option<CompletionSpec>,
    pub empty_line: Option<CompletionSpec>,
    pub initial_word: Option<CompletionSpec>,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, Default)]
pub struct CompletionSpec {
    //
    // Options
    //
    pub bash_default: bool,
    pub default: bool,
    pub dir_names: bool,
    pub file_names: bool,
    pub no_quote: bool,
    pub no_sort: bool,
    pub no_space: bool,
    pub plus_dirs: bool,

    //
    // Generators
    //
    pub actions: Vec<CompleteAction>,
    pub glob_pattern: Option<String>,
    pub word_list: Option<String>,
    pub function_name: Option<String>,
    pub command: Option<String>,

    //
    // Filters
    //
    pub filter_pattern: Option<String>,
    pub filter_pattern_excludes: bool,

    //
    // Transformers
    //
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct CompletionContext<'a> {
    /// The token to complete.
    pub token_to_complete: &'a str,

    /// Other potentially relevant tokens.
    pub command_name: Option<&'a str>,
    pub preceding_token: Option<&'a str>,

    /// The index of the token to complete.
    pub token_index: usize,

    /// The input.
    pub input_line: &'a str,
    pub cursor_index: usize,
    pub tokens: &'a [&'a parser::Token],
}

impl CompletionSpec {
    pub async fn get_completions(
        &self,
        shell: &mut Shell,
        context: &CompletionContext<'_>,
    ) -> Result<CompletionResult, crate::error::Error> {
        let mut candidates: Vec<String> = vec![];

        // Apply options
        if self.bash_default {
            log::debug!("UNIMPLEMENTED: complete -o bashdefault");
        }
        if self.default {
            log::debug!("UNIMPLEMENTED: complete -o default");
        }
        if self.dir_names {
            log::debug!("UNIMPLEMENTED: complete -o dirnames");
        }
        if self.file_names {
            log::debug!("UNIMPLEMENTED: complete -o filenames");
        }
        if self.no_quote {
            log::debug!("UNIMPLEMENTED: complete -o noquote");
        }
        if self.no_space {
            log::debug!("UNIMPLEMENTED: complete -o nospace");
        }
        if self.plus_dirs {
            log::debug!("UNIMPLEMENTED: complete -o plusdirs");
        }

        for action in &self.actions {
            match action {
                CompleteAction::Alias => log::debug!("UNIMPLEMENTED: complete -A alias"),
                CompleteAction::ArrayVar => log::debug!("UNIMPLEMENTED: complete -A arrayvar"),
                CompleteAction::Binding => log::debug!("UNIMPLEMENTED: complete -A binding"),
                CompleteAction::Builtin => log::debug!("UNIMPLEMENTED: complete -A builtin"),
                CompleteAction::Command => log::debug!("UNIMPLEMENTED: complete -A command"),
                CompleteAction::Directory => log::debug!("UNIMPLEMENTED: complete -A directory"),
                CompleteAction::Disabled => log::debug!("UNIMPLEMENTED: complete -A disabled"),
                CompleteAction::Enabled => log::debug!("UNIMPLEMENTED: complete -A enabled"),
                CompleteAction::Export => log::debug!("UNIMPLEMENTED: complete -A export"),
                CompleteAction::File => log::debug!("UNIMPLEMENTED: complete -A file"),
                CompleteAction::Function => log::debug!("UNIMPLEMENTED: complete -A function"),
                CompleteAction::Group => log::debug!("UNIMPLEMENTED: complete -A group"),
                CompleteAction::HelpTopic => log::debug!("UNIMPLEMENTED: complete -A helptopic"),
                CompleteAction::HostName => log::debug!("UNIMPLEMENTED: complete -A hostname"),
                CompleteAction::Job => log::debug!("UNIMPLEMENTED: complete -A job"),
                CompleteAction::Keyword => log::debug!("UNIMPLEMENTED: complete -A keyword"),
                CompleteAction::Running => log::debug!("UNIMPLEMENTED: complete -A running"),
                CompleteAction::Service => log::debug!("UNIMPLEMENTED: complete -A service"),
                CompleteAction::SetOpt => log::debug!("UNIMPLEMENTED: complete -A setopt"),
                CompleteAction::ShOpt => log::debug!("UNIMPLEMENTED: complete -A shopt"),
                CompleteAction::Signal => log::debug!("UNIMPLEMENTED: complete -A signal"),
                CompleteAction::Stopped => log::debug!("UNIMPLEMENTED: complete -A stopped"),
                CompleteAction::User => log::debug!("UNIMPLEMENTED: complete -A user"),
                CompleteAction::Variable => {
                    shell.env.iter().for_each(|(key, _value)| {
                        candidates.push(key.to_string());
                    });
                }
            }
        }

        if let Some(glob_pattern) = &self.glob_pattern {
            log::debug!("UNIMPLEMENTED: complete -G({glob_pattern})");
        }
        if let Some(word_list) = &self.word_list {
            log::debug!("UNIMPLEMENTED: complete -W({word_list})");
        }
        if let Some(function_name) = &self.function_name {
            let call_result = self
                .call_completion_function(shell, function_name.as_str(), context)
                .await?;
            match call_result {
                CompletionResult::RestartCompletionProcess => return Ok(call_result),
                CompletionResult::Candidates(mut new_candidates) => {
                    candidates.append(&mut new_candidates);
                }
            }
        }
        if let Some(command) = &self.command {
            log::debug!("UNIMPLEMENTED: complete -C({command})");
        }

        // Make sure the token we have (if non-empty) is a prefix.
        if !context.token_to_complete.is_empty() {
            candidates.retain(|candidate| candidate.starts_with(context.token_to_complete));
        }

        // Apply filter pattern, if present.
        if let Some(filter_pattern) = &self.filter_pattern {
            log::debug!("UNIMPLEMENTED: complete -X (filter pattern): {filter_pattern}");
        }

        // Add prefix and/or suffix, if present.
        if let Some(prefix) = &self.prefix {
            for candidate in &mut candidates {
                candidate.insert_str(0, prefix);
            }
        }
        if let Some(suffix) = &self.suffix {
            for candidate in &mut candidates {
                candidate.push_str(suffix);
            }
        }

        // Sort, if desired.
        if !self.no_sort {
            candidates.sort();
        }

        Ok(CompletionResult::Candidates(candidates))
    }

    async fn call_completion_function(
        &self,
        shell: &mut Shell,
        function_name: &str,
        context: &CompletionContext<'_>,
    ) -> Result<CompletionResult, error::Error> {
        // TODO: Don't pollute the persistent environment with these.
        let vars_and_values: Vec<(&str, ShellValueLiteral)> = vec![
            ("COMP_LINE", context.input_line.into()),
            ("COMP_POINT", context.cursor_index.to_string().into()),
            // TODO: ("COMP_KEY", String::from("???")),
            // TODO: ("COMP_TYPE", String::from("???")),
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

        for (var, value) in vars_and_values {
            shell.env.update_or_add(
                var,
                value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;
        }

        let mut args = vec![
            context.command_name.unwrap_or(""),
            context.token_to_complete,
        ];
        if let Some(preceding_token) = context.preceding_token {
            args.push(preceding_token);
        }

        let result = shell.invoke_function(function_name, &args).await?;

        // When the function returns the special value 124, then it's a request
        // for us to restart the completion process.
        if result == 124 {
            Ok(CompletionResult::RestartCompletionProcess)
        } else {
            if let Some((_, reply)) = shell.env.get("COMPREPLY") {
                match reply.value() {
                    crate::variables::ShellValue::IndexedArray(values) => {
                        Ok(CompletionResult::Candidates(
                            values.values().map(|v| v.to_owned()).collect(),
                        ))
                    }
                    _ => error::unimp("unexpected COMPREPLY value type"),
                }
            } else {
                Ok(CompletionResult::Candidates(vec![]))
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Completions {
    pub start: usize,
    pub candidates: Vec<String>,
}

#[allow(clippy::module_name_repetitions)]
pub enum CompletionResult {
    Candidates(Vec<String>),
    RestartCompletionProcess,
}

impl CompletionConfig {
    #[allow(clippy::cast_sign_loss)]
    pub async fn get_completions(
        &self,
        shell: &mut Shell,
        input: &str,
        position: usize,
    ) -> Result<Completions, error::Error> {
        const MAX_RESTARTS: u32 = 10;

        // Make a best-effort attempt to tokenize.
        if let Ok(tokens) = parser::tokenize_str(input) {
            let cursor: i32 = i32::try_from(position)?;
            let mut preceding_token = None;
            let mut completion_prefix = "";
            let mut insertion_point = cursor;
            let mut completion_token_index = tokens.len();

            // Copy a set of references to the tokens; we will adjust this list as
            // we find we need to insert an empty token.
            let mut adjusted_tokens: Vec<&parser::Token> = tokens.iter().collect();

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
                else if cursor >= token.location().start.index
                    && cursor <= token.location().end.index
                {
                    // Update insertion point.
                    insertion_point = token.location().start.index;

                    // Update prefix.
                    let offset_into_token = (cursor - insertion_point) as usize;
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
            let empty_token = parser::Token::Word(String::new(), parser::TokenLocation::default());
            adjusted_tokens.push(&empty_token);

            // Get the completions.
            let mut result = CompletionResult::RestartCompletionProcess;
            let mut restart_count = 0;
            while matches!(result, CompletionResult::RestartCompletionProcess) {
                if restart_count > MAX_RESTARTS {
                    log::error!("possible infinite loop detected in completion process");
                    break;
                }

                let completion_context = CompletionContext {
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

            let candidates = match result {
                CompletionResult::Candidates(candidates) => candidates,
                CompletionResult::RestartCompletionProcess => vec![],
            };

            Ok(Completions {
                start: insertion_point as usize,
                candidates,
            })
        } else {
            Ok(Completions {
                start: position,
                candidates: vec![],
            })
        }
    }

    #[allow(clippy::unused_self)]
    async fn get_completions_for_token<'a>(
        &self,
        shell: &mut Shell,
        mut context: CompletionContext<'a>,
    ) -> CompletionResult {
        // N.B. We basic-expand the token-to-be-completed first.
        let mut throwaway_shell = shell.clone();
        let expanded_token_to_complete = throwaway_shell
            .basic_expand_string(context.token_to_complete)
            .await
            .unwrap_or_else(|_| context.token_to_complete.to_owned());
        context.token_to_complete = expanded_token_to_complete.as_str();

        // See if we can find a completion spec matching the current command.
        let mut found_spec: Option<&CompletionSpec> = None;
        if let Some(command_name) = context.command_name {
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
        } else {
            if let Some(spec) = &self.empty_line {
                found_spec = Some(spec);
            }
        }

        if found_spec.is_none() {
            if let Some(spec) = &self.default {
                found_spec = Some(spec);
            }
        }

        // Try to generate completions.
        if let Some(spec) = found_spec {
            let result = spec
                .to_owned()
                .get_completions(shell, &context)
                .await
                .unwrap_or_else(|_err| CompletionResult::Candidates(vec![]));

            if !matches!(&result, CompletionResult::Candidates(candidates) if candidates.is_empty())
            {
                return result;
            }
        }

        self.get_completions_using_basic_lookup(shell, &context)
    }

    #[allow(clippy::unused_self)]
    fn get_completions_using_basic_lookup(
        &self,
        shell: &Shell,
        context: &CompletionContext,
    ) -> CompletionResult {
        // TODO: Contextually generate different completions.
        let glob = std::format!("{}*", context.token_to_complete);
        // TODO: Pass through quoting.
        let mut candidates = if let Ok(mut candidates) = patterns::Pattern::from(glob)
            .expand(shell.working_dir.as_path(), shell.options.extended_globbing)
        {
            for candidate in &mut candidates {
                if Path::new(candidate.as_str()).is_dir() {
                    candidate.push('/');
                }
            }
            candidates
        } else {
            vec![]
        };

        // If this appears to be the command token (and if there's *some* prefix without
        // a path separator) then also consider whether we should search the path for
        // completions too.
        // TODO: Do a better job than just checking if index == 0.
        if context.token_index == 0
            && !context.token_to_complete.is_empty()
            && !context.token_to_complete.contains('/')
        {
            let glob_pattern = std::format!("{}*", context.token_to_complete);

            for path in shell.find_executables_in_path(&glob_pattern) {
                if let Some(file_name) = path.file_name() {
                    candidates.push(file_name.to_string_lossy().to_string());
                }
            }
        }

        if context.token_index + 1 >= context.tokens.len() {
            for candidate in &mut candidates {
                if !candidate.ends_with('/') {
                    candidate.push(' ');
                }
            }
        }

        #[cfg(windows)]
        {
            candidates = candidates
                .into_iter()
                .map(|c| c.replace("\\", "/"))
                .collect();
        }

        CompletionResult::Candidates(candidates)
    }
}
