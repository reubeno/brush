use anyhow::Result;
use faccess::PathExt;
use log::debug;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::env::{EnvironmentLookup, EnvironmentScope, ShellEnvironment};
use crate::error;
use crate::expansion;
use crate::interp::{Execute, ExecutionParameters, ExecutionResult};
use crate::jobs;
use crate::options::RuntimeOptions;
use crate::patterns;
use crate::prompt::expand_prompt;
use crate::variables::{self, ShellValue, ShellVariable};

pub struct Shell {
    // TODO: open files
    pub working_dir: PathBuf,
    pub umask: u32,
    pub file_size_limit: u64,
    // TODO: traps
    pub env: ShellEnvironment,
    pub funcs: HashMap<String, ShellFunction>,
    pub options: RuntimeOptions,
    pub jobs: jobs::JobManager,
    pub aliases: HashMap<String, String>,

    //
    // Additional state
    //
    pub last_exit_status: u8,

    // Positional parameters ($1 and beyond)
    pub positional_parameters: Vec<String>,

    // Shell name
    pub shell_name: Option<String>,

    // Function call stack.
    pub function_call_depth: u32,

    // Directory stack used by pushd et al.
    pub directory_stack: Vec<PathBuf>,
}

impl Clone for Shell {
    fn clone(&self) -> Self {
        Self {
            working_dir: self.working_dir.clone(),
            umask: self.umask,
            file_size_limit: self.file_size_limit,
            env: self.env.clone(),
            funcs: self.funcs.clone(),
            options: self.options.clone(),
            jobs: jobs::JobManager::new(),
            aliases: self.aliases.clone(),
            last_exit_status: self.last_exit_status,
            positional_parameters: self.positional_parameters.clone(),
            shell_name: self.shell_name.clone(),
            function_call_depth: self.function_call_depth,
            directory_stack: self.directory_stack.clone(),
        }
    }
}

#[derive(Debug)]
pub struct CreateOptions {
    pub login: bool,
    pub interactive: bool,
    pub no_editing: bool,
    pub no_profile: bool,
    pub no_rc: bool,
    pub posix: bool,
    pub print_commands_and_arguments: bool,
    pub shell_name: Option<String>,
    pub verbose: bool,
}

type ShellFunction = parser::ast::FunctionDefinition;

#[derive(Debug)]
pub struct FunctionCall {}

pub enum ProgramOrigin {
    File(PathBuf),
    String,
}

impl ProgramOrigin {
    fn get_name(&self) -> String {
        match self {
            ProgramOrigin::File(path) => path.to_string_lossy().to_string(),
            ProgramOrigin::String => "<string>".to_owned(),
        }
    }
}

impl std::fmt::Display for ProgramOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_name())
    }
}

#[derive(Debug, Default)]
pub struct Completions {
    pub start: usize,
    pub candidates: Vec<String>,
}

impl Shell {
    pub async fn new(options: &CreateOptions) -> Result<Shell> {
        // Instantiate the shell with some defaults.
        let mut shell = Shell {
            working_dir: std::env::current_dir()?,
            umask: Default::default(),           // TODO: populate umask
            file_size_limit: Default::default(), // TODO: populate file size limit
            env: Self::initialize_vars(options),
            funcs: HashMap::default(),
            options: RuntimeOptions::defaults_from(options),
            jobs: jobs::JobManager::new(),
            aliases: HashMap::default(),
            last_exit_status: 0,
            positional_parameters: vec![],
            shell_name: options.shell_name.clone(),
            function_call_depth: 0,
            directory_stack: vec![],
        };

        // TODO: Figure out how this got hard-coded.
        shell.options.extended_globbing = true;

        // Load profiles/configuration.
        shell.load_config(options).await?;

        Ok(shell)
    }

    fn initialize_vars(options: &CreateOptions) -> ShellEnvironment {
        let mut env = ShellEnvironment::new();

        // Seed parameters from environment.
        for (k, v) in std::env::vars() {
            let mut var = ShellVariable::new(ShellValue::String(v));
            var.export();
            env.set_global(k, var);
        }

        // Set some additional ones.
        let mut euid_var = ShellVariable::new(ShellValue::String(format!(
            "{}",
            uzers::get_effective_uid()
        )));
        euid_var.set_readonly();
        env.set_global("EUID", euid_var);

        let mut random_var = ShellVariable::new(ShellValue::Random);
        random_var.hide_from_enumeration();
        env.set_global("RANDOM", random_var);

        // Set some defaults (if they're not already initialized).
        if !env.is_set("HISTFILE") {
            if let Some(home_dir) = env.get("HOME") {
                let home_dir: String = home_dir.value().into();
                let home_dir = PathBuf::from(home_dir);
                let histfile = home_dir.join(".brush_history");
                env.set_global(
                    "HISTFILE",
                    ShellVariable::new(ShellValue::String(histfile.to_string_lossy().to_string())),
                );
            }
        }
        if !env.is_set("PATH") {
            env.set_global(
                "PATH",
                ShellVariable::new(
                    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
                ),
            );
        }

        // TODO: don't set these in sh mode
        if let Some(shell_name) = &options.shell_name {
            env.set_global("BASH", ShellVariable::new(shell_name.into()));
        }
        env.set_global(
            "BASH_VERSINFO",
            ShellVariable::new(ShellValue::indexed_array_from_slice(
                ["5", "1", "1", "1", "release", "unknown"].as_slice(),
            )),
        );

        env
    }

    async fn load_config(&mut self, options: &CreateOptions) -> Result<()> {
        if options.login {
            // --noprofile means skip this.
            if options.no_profile {
                return Ok(());
            }

            //
            // Source /etc/profile if it exists.
            //
            // Next source the first of these that exists and is readable (if any):
            //     * ~/.bash_profile
            //     * ~/.bash_login
            //     * ~/.profile
            //
            self.source_if_exists(Path::new("/etc/profile")).await?;
            if let Ok(home_path) = std::env::var("HOME") {
                if !self
                    .source_if_exists(Path::new(&home_path).join(".bash_profile").as_path())
                    .await?
                {
                    if !self
                        .source_if_exists(Path::new(&home_path).join(".bash_login").as_path())
                        .await?
                    {
                        self.source_if_exists(Path::new(&home_path).join(".profile").as_path())
                            .await?;
                    }
                }
            }
        } else {
            if options.interactive {
                // --norc means skip this.
                if options.no_rc {
                    return Ok(());
                }

                //
                // For non-login interactive shells, load in this order:
                //
                //     /etc/bash.bashrc
                //     ~/.bashrc
                //
                self.source_if_exists(Path::new("/etc/bash.bashrc")).await?;
                if let Ok(home_path) = std::env::var("HOME") {
                    self.source_if_exists(Path::new(&home_path).join(".bashrc").as_path())
                        .await?;
                    self.source_if_exists(Path::new(&home_path).join(".brushrc").as_path())
                        .await?;
                }
            } else {
                if self.env.is_set("BASH_ENV") {
                    //
                    // TODO: look at $BASH_ENV; source its expansion if that file exists
                    //
                    return Err(anyhow::anyhow!("UNIMPLEMENTED: load config from $BASH_ENV for non-interactive, non-login shell"));
                }
            }
        }

        Ok(())
    }

    async fn source_if_exists(&mut self, path: &Path) -> Result<bool> {
        if path.exists() {
            let args: Vec<String> = vec![];
            self.source(path, &args).await?;
            Ok(true)
        } else {
            debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    pub async fn source<S: AsRef<str>>(
        &mut self,
        path: &Path,
        args: &[S],
    ) -> Result<ExecutionResult, error::Error> {
        log::debug!("sourcing: {}", path.display());

        let opened_file = std::fs::File::open(path)
            .map_err(|e| error::Error::FailedSourcingFile(path.to_owned(), e))?;

        let file_metadata = opened_file
            .metadata()
            .map_err(|e| error::Error::FailedSourcingFile(path.to_owned(), e))?;

        if file_metadata.is_dir() {
            return Err(error::Error::FailedSourcingFile(
                path.to_owned(),
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    anyhow::anyhow!("path is a directory"),
                ),
            ));
        }

        let origin = ProgramOrigin::File(path.to_owned());

        self.source_file(&opened_file, &origin, args).await
    }

    pub async fn source_file<S: AsRef<str>>(
        &mut self,
        file: &std::fs::File,
        origin: &ProgramOrigin,
        args: &[S],
    ) -> Result<ExecutionResult, error::Error> {
        let mut reader = std::io::BufReader::new(file);
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options());
        let parse_result = parser.parse(false);

        // TODO: Find a cleaner way to change args.
        let orig_shell_name = self.shell_name.take();
        let orig_params = self.positional_parameters.clone();
        self.shell_name = Some(origin.get_name());
        self.positional_parameters = vec![];

        // TODO: handle args
        if !args.is_empty() {
            log::error!("UNIMPLEMENTED: source built-in invoked with args: {origin}",);
        }

        let result = self.run_parsed_result(parse_result, origin, false).await;

        // Restore.
        self.shell_name = orig_shell_name;
        self.positional_parameters = orig_params;

        result
    }

    pub async fn run_string(
        &mut self,
        command: &str,
        capture_output: bool,
    ) -> Result<ExecutionResult, error::Error> {
        let parse_result = self.parse_string(command);
        self.run_parsed_result(parse_result, &ProgramOrigin::String, capture_output)
            .await
    }

    pub fn parse_string<S: AsRef<str>>(
        &self,
        s: S,
    ) -> Result<parser::ast::Program, parser::ParseError> {
        let mut reader = std::io::BufReader::new(s.as_ref().as_bytes());
        let mut parser = parser::Parser::new(&mut reader, &self.parser_options());
        parser.parse(true)
    }

    pub async fn run_script<S: AsRef<str>>(
        &mut self,
        script_path: &Path,
        args: &[S],
    ) -> Result<ExecutionResult, error::Error> {
        self.source(script_path, args).await
    }

    async fn run_parsed_result(
        &mut self,
        parse_result: Result<parser::ast::Program, parser::ParseError>,
        origin: &ProgramOrigin,
        capture_output: bool,
    ) -> Result<ExecutionResult, error::Error> {
        let mut error_prefix = String::new();

        if let ProgramOrigin::File(file_path) = origin {
            error_prefix = format!("{}: ", file_path.display());
        }

        let result = match parse_result {
            Ok(prog) => self.run_program(prog, capture_output).await?,
            Err(parser::ParseError::ParsingNearToken(token_near_error)) => {
                let error_loc = &token_near_error.location().start;

                log::error!(
                    "{}syntax error near token `{}' (line {} col {})",
                    error_prefix,
                    token_near_error.to_str(),
                    error_loc.line,
                    error_loc.column,
                );
                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(parser::ParseError::ParsingAtEndOfInput) => {
                log::error!("{}syntax error at end of input", error_prefix);
                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
            Err(parser::ParseError::Tokenizing { inner, position }) => {
                let mut error_message = error_prefix.clone();
                error_message.push_str(inner.to_string().as_str());

                if let Some(position) = position {
                    error_message.push_str(&format!(
                        " (detected near line {} column {})",
                        position.line, position.column
                    ));
                }

                log::error!("{}", error_message);

                self.last_exit_status = 2;
                ExecutionResult::new(2)
            }
        };

        Ok(result)
    }

    pub async fn run_program(
        &mut self,
        program: parser::ast::Program,
        capture_output: bool,
    ) -> Result<ExecutionResult, error::Error> {
        program
            .execute(self, &ExecutionParameters { capture_output })
            .await
    }

    pub async fn compose_prompt(&mut self) -> Result<String> {
        const DEFAULT_PROMPT: &str = "$ ";

        // Retrieve the spec.
        let ps1 = self.parameter_or_default("PS1", DEFAULT_PROMPT);

        // Expand it.
        let formatted_prompt = expand_prompt(self, ps1.as_str())?;

        // NOTE: We're having difficulty with xterm escape sequences going through rustyline;
        // so we strip them here.
        let re = regex::Regex::new("\x1b][0-2];[^\x07]*\x07")?;
        let formatted_prompt = re.replace_all(formatted_prompt.as_str(), "").to_string();

        // Now expand.
        let formatted_prompt = expansion::basic_expand_word_str(self, &formatted_prompt).await?;

        Ok(formatted_prompt)
    }

    pub fn last_result(&self) -> u8 {
        self.last_exit_status
    }

    fn parameter_or_default(&self, name: &str, default: &str) -> String {
        self.env
            .get(name)
            .map_or_else(|| default.to_owned(), |s| String::from(s.value()))
    }

    pub fn current_option_flags(&self) -> String {
        let mut cs = vec![];

        for (x, y) in crate::namedoptions::SET_OPTIONS.iter() {
            if (y.getter)(self) {
                cs.push(*x);
            }
        }

        cs.into_iter().collect()
    }

    fn parser_options(&self) -> parser::ParserOptions {
        parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
        }
    }

    pub fn in_function(&self) -> bool {
        self.function_call_depth > 0
    }

    pub fn enter_function(&mut self) {
        self.function_call_depth += 1;
        self.env.push_locals();
    }

    pub fn leave_function(&mut self) {
        self.env.pop_locals();
        self.function_call_depth -= 1;
    }

    pub fn get_history_file_path(&self) -> Option<PathBuf> {
        self.env.get("HISTFILE").map(|var| {
            let histfile_str: String = (var.value()).into();
            PathBuf::from(histfile_str)
        })
    }

    #[allow(clippy::cast_sign_loss)]
    pub fn get_completions(&self, input: &str, position: usize) -> Result<Completions> {
        // Make a best-effort attempt to tokenize.
        if let Ok(result) = parser::tokenize_str(input) {
            let cursor: i32 = i32::try_from(position)?;
            let mut completion_prefix = "";
            let mut insertion_point = cursor;
            let mut completion_token_index = result.len();

            // Try to find which token we are in.
            for (i, token) in result.iter().enumerate() {
                // If the cursor is before the start of the token, then it's between
                // this token and the one that preceded it (or it's before the first
                // token if this is the first token).
                if cursor < token.location().start.index {
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

                // Otherwise, we need to keep looking.
            }

            Ok(Completions {
                start: insertion_point as usize,
                candidates: self.get_completions_with_prefix(
                    completion_prefix,
                    completion_token_index,
                    result.len(),
                ),
            })
        } else {
            Ok(Completions {
                start: position,
                candidates: vec![],
            })
        }
    }

    fn get_completions_with_prefix(
        &self,
        prefix: &str,
        token_index: usize,
        token_count: usize,
    ) -> Vec<String> {
        // TODO: Contextually generate different completions.
        let glob = std::format!("{prefix}*");
        let mut candidates = if let Ok(candidates) =
            patterns::pattern_expand(glob.as_str(), self.working_dir.as_path())
        {
            candidates
                .into_iter()
                .map(|p| {
                    let mut s = p.to_string_lossy().to_string();
                    if p.is_dir() {
                        s.push('/');
                    }
                    s
                })
                .collect()
        } else {
            vec![]
        };

        // TODO: Do a better job than just checking if index == 0.
        if token_index == 0 && !prefix.is_empty() {
            let glob_pattern = std::format!("{prefix}*");

            for path in self.find_executables_in_path(&glob_pattern) {
                if let Some(file_name) = path.file_name() {
                    candidates.push(file_name.to_string_lossy().to_string());
                }
            }
        }

        if token_index + 1 >= token_count {
            for candidate in &mut candidates {
                if !candidate.ends_with('/') {
                    candidate.push(' ');
                }
            }
        }

        candidates
    }

    #[allow(clippy::manual_flatten)]
    pub fn find_executables_in_path(&self, required_glob_pattern: &str) -> Vec<PathBuf> {
        let mut executables = vec![];
        for dir_str in self.env.get_str("PATH").unwrap_or_default().split(':') {
            if let Ok(entries) =
                glob::glob(std::format!("{dir_str}/{required_glob_pattern}").as_str())
            {
                for entry in entries {
                    if let Ok(entry) = entry {
                        if entry.executable() {
                            executables.push(entry);
                        }
                    }
                }
            }
        }

        executables
    }

    pub fn set_working_dir(&mut self, target_dir: &Path) -> Result<(), error::Error> {
        let abs_path = if target_dir.is_absolute() {
            PathBuf::from(target_dir)
        } else {
            self.working_dir.join(target_dir)
        };

        match std::fs::metadata(&abs_path) {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::Error::NotADirectory(abs_path));
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // TODO: Don't canonicalize, just normalize.
        let cleaned_path = abs_path.canonicalize()?;

        let pwd = cleaned_path.to_string_lossy().to_string();

        // TODO: handle updating PWD
        self.working_dir = cleaned_path;
        self.env.update_or_add(
            "PWD",
            variables::ShellValueLiteral::Scalar(pwd),
            |var| {
                var.export();
                Ok(())
            },
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    pub fn tilde_shorten(&self, s: String) -> String {
        let home_dir_opt = self.env.get("HOME");
        if let Some(home_dir) = home_dir_opt {
            if let Some(stripped) = s.strip_prefix(&String::from(home_dir.value())) {
                return format!("~{stripped}");
            }
        }
        s
    }
}
