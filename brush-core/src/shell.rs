use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use normalize_path::NormalizePath;
use tokio::sync::Mutex;

use crate::arithmetic::Evaluatable;
use crate::env::{EnvironmentLookup, EnvironmentScope, ShellEnvironment};
use crate::interp::{Execute, ExecutionParameters};
use crate::options::RuntimeOptions;
use crate::results::ExecutionWaitResult;
use crate::sys::fs::PathExt;
use crate::variables::{self, ShellVariable};
use crate::{
    CreateOptions, DefaultShellBehavior, ExecutionControlFlow, ExecutionResult, ProcessGroupPolicy,
    ProfileLoadBehavior, RcLoadBehavior, ShellBehavior, ShellRuntime, callstack, history,
    interfaces, pathcache, pathsearch, trace_categories, wellknownvars,
};
use crate::{
    builtins, commands, completion, env, error, expansion, functions, jobs, keywords, openfiles,
    prompt, sys::users, traps,
};

/// Type for storing a key bindings helper.
pub type KeyBindingsHelper = Arc<Mutex<dyn interfaces::KeyBindings>>;

/// Type alias for shell file descriptors.
pub type ShellFd = i32;

/// Default shell runtime type.
pub type DefaultShellRuntime = Shell;

/// Represents an instance of a shell.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Shell<SB: ShellBehavior = DefaultShellBehavior> {
    /// Injected shell behavior.
    #[cfg_attr(feature = "serde", serde(skip, default = "default_shell_behavior"))]
    behavior: SB,

    /// Trap handler configuration for the shell.
    traps: traps::TrapHandlerConfig,

    /// Manages files opened and accessible via redirection operators.
    open_files: openfiles::OpenFiles,

    /// The current working directory.
    working_dir: PathBuf,

    /// The shell environment, containing shell variables.
    env: ShellEnvironment,

    /// Shell function definitions.
    funcs: functions::FunctionEnv,

    /// Runtime shell options.
    options: RuntimeOptions,

    /// State of managed jobs.
    /// TODO(serde): Need to warn somehow that jobs cannot be serialized.
    #[cfg_attr(feature = "serde", serde(skip))]
    jobs: jobs::JobManager,

    /// Shell aliases.
    aliases: HashMap<String, String>,

    /// The status of the last completed command.
    last_exit_status: u8,

    /// Tracks changes to `last_exit_status`.
    last_exit_status_change_count: usize,

    /// The status of each of the commands in the last pipeline.
    last_pipeline_statuses: Vec<u8>,

    /// Clone depth from the original ancestor shell.
    depth: usize,

    /// Shell name
    name: Option<String>,

    /// Positional shell arguments (not including shell name).
    args: Vec<String>,

    /// Shell version
    version: Option<String>,

    /// Detailed display string for the shell
    product_display_str: Option<String>,

    /// Function/script call stack.
    call_stack: callstack::CallStack,

    /// Directory stack used by pushd et al.
    directory_stack: Vec<PathBuf>,

    /// Completion configuration.
    completion_config: completion::Config,

    /// Shell built-in commands.
    #[cfg_attr(feature = "serde", serde(skip))]
    builtins: HashMap<String, builtins::Registration<Self>>,

    /// Shell program location cache.
    program_location_cache: pathcache::PathCache,

    /// Last "SECONDS" captured time.
    last_stopwatch_time: std::time::SystemTime,

    /// Last "SECONDS" offset requested.
    last_stopwatch_offset: u32,

    /// Key bindings for the shell, optionally implemented by an interactive shell.
    #[cfg_attr(feature = "serde", serde(skip))]
    key_bindings: Option<KeyBindingsHelper>,

    /// History of commands executed in the shell.
    history: Option<history::History>,
}

impl<SB: ShellBehavior> Clone for Shell<SB> {
    fn clone(&self) -> Self {
        Self {
            behavior: self.behavior.clone(),
            traps: self.traps.clone(),
            open_files: self.open_files.clone(),
            working_dir: self.working_dir.clone(),
            env: self.env.clone(),
            funcs: self.funcs.clone(),
            options: self.options.clone(),
            jobs: jobs::JobManager::new(),
            aliases: self.aliases.clone(),
            last_exit_status: self.last_exit_status,
            last_exit_status_change_count: self.last_exit_status_change_count,
            last_pipeline_statuses: self.last_pipeline_statuses.clone(),
            name: self.name.clone(),
            args: self.args.clone(),
            version: self.version.clone(),
            product_display_str: self.product_display_str.clone(),
            call_stack: self.call_stack.clone(),
            directory_stack: self.directory_stack.clone(),
            completion_config: self.completion_config.clone(),
            builtins: self.builtins.clone(),
            program_location_cache: self.program_location_cache.clone(),
            last_stopwatch_time: self.last_stopwatch_time,
            last_stopwatch_offset: self.last_stopwatch_offset,
            key_bindings: self.key_bindings.clone(),
            history: self.history.clone(),
            depth: self.depth + 1,
        }
    }
}

impl<SB: ShellBehavior> AsRef<Self> for Shell<SB> {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl<SB: ShellBehavior> AsMut<Self> for Shell<SB> {
    fn as_mut(&mut self) -> &mut Self {
        self
    }
}

#[async_trait]
impl<SB: ShellBehavior> ShellRuntime for Shell<SB> {
    fn clone_subshell(&self) -> Self {
        <Self as Clone>::clone(self)
    }

    fn version(&self) -> &Option<String> {
        &self.version
    }

    fn call_stack(&self) -> &callstack::CallStack {
        &self.call_stack
    }

    fn last_exit_status(&self) -> u8 {
        self.last_exit_status
    }

    fn set_last_exit_status(&mut self, status: u8) {
        self.last_exit_status = status;
        self.last_exit_status_change_count += 1;
    }

    fn key_bindings(&self) -> &Option<KeyBindingsHelper> {
        &self.key_bindings
    }

    fn set_key_bindings(&mut self, key_bindings: Option<KeyBindingsHelper>) {
        self.key_bindings = key_bindings;
    }

    fn builtins(&self) -> &HashMap<String, builtins::Registration<Self>> {
        &self.builtins
    }

    fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    fn product_display_str(&self) -> &Option<String> {
        &self.product_display_str
    }

    fn funcs(&self) -> &functions::FunctionEnv {
        &self.funcs
    }

    fn funcs_mut(&mut self) -> &mut functions::FunctionEnv {
        &mut self.funcs
    }

    fn env(&self) -> &ShellEnvironment {
        &self.env
    }

    fn env_mut(&mut self) -> &mut ShellEnvironment {
        &mut self.env
    }

    fn options(&self) -> &RuntimeOptions {
        &self.options
    }

    fn options_mut(&mut self) -> &mut RuntimeOptions {
        &mut self.options
    }

    fn aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    fn aliases_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.aliases
    }

    fn jobs(&self) -> &jobs::JobManager {
        &self.jobs
    }

    fn jobs_mut(&mut self) -> &mut jobs::JobManager {
        &mut self.jobs
    }

    fn traps(&self) -> &traps::TrapHandlerConfig {
        &self.traps
    }

    fn traps_mut(&mut self) -> &mut traps::TrapHandlerConfig {
        &mut self.traps
    }

    fn directory_stack(&self) -> &[PathBuf] {
        &self.directory_stack
    }

    fn directory_stack_mut(&mut self) -> &mut Vec<PathBuf> {
        &mut self.directory_stack
    }

    fn last_pipeline_statuses(&self) -> &[u8] {
        &self.last_pipeline_statuses
    }

    fn last_pipeline_statuses_mut(&mut self) -> &mut Vec<u8> {
        &mut self.last_pipeline_statuses
    }

    fn program_location_cache(&self) -> &pathcache::PathCache {
        &self.program_location_cache
    }

    fn program_location_cache_mut(&mut self) -> &mut pathcache::PathCache {
        &mut self.program_location_cache
    }

    fn completion_config(&self) -> &completion::Config {
        &self.completion_config
    }

    fn completion_config_mut(&mut self) -> &mut completion::Config {
        &mut self.completion_config
    }

    fn open_files(&self) -> &openfiles::OpenFiles {
        &self.open_files
    }

    fn open_files_mut(&mut self) -> &mut openfiles::OpenFiles {
        &mut self.open_files
    }

    fn last_stopwatch_time(&self) -> std::time::SystemTime {
        self.last_stopwatch_time
    }

    fn last_stopwatch_offset(&self) -> u32 {
        self.last_stopwatch_offset
    }

    fn history(&self) -> Option<&history::History> {
        self.history.as_ref()
    }

    fn history_mut(&mut self) -> Option<&mut history::History> {
        self.history.as_mut()
    }

    fn is_subshell(&self) -> bool {
        self.depth > 0
    }

    fn depth(&self) -> usize {
        self.depth
    }

    fn env_str(&self, name: &str) -> Option<Cow<'_, str>> {
        self.env.get_str(name, self)
    }

    fn env_var(&self, name: &str) -> Option<&ShellVariable> {
        self.env.get(name).map(|(_, var)| var)
    }

    fn current_shell_name(&self) -> Option<Cow<'_, str>> {
        for frame in self.call_stack.iter() {
            // Executed scripts shadow the shell name.
            if frame.frame_type.is_run_script() {
                return Some(frame.frame_type.name());
            }
        }

        self.name.as_deref().map(|name| name.into())
    }

    fn current_shell_args(&self) -> &[String] {
        for frame in self.call_stack.iter() {
            match frame.frame_type {
                // Function calls always shadow positional parameters.
                callstack::FrameType::Function(..) => return &frame.args,
                // Executed scripts always shadow positional parameters.
                _ if frame.frame_type.is_run_script() => return &frame.args,
                // Sourced scripts shadow positional parameters if they have arguments.
                _ if frame.frame_type.is_sourced_script() => {
                    if !frame.args.is_empty() {
                        return &frame.args;
                    }
                }
                _ => (),
            }
        }

        self.args.as_slice()
    }

    fn ifs(&self) -> Cow<'_, str> {
        self.env_str("IFS").unwrap_or_else(|| " \t\n".into())
    }

    fn get_ifs_first_char(&self) -> char {
        self.ifs().chars().next().unwrap_or(' ')
    }

    fn get_keywords(&self) -> Vec<String> {
        if self.options.sh_mode {
            keywords::SH_MODE_KEYWORDS.iter().cloned().collect()
        } else {
            keywords::KEYWORDS.iter().cloned().collect()
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        if let Some(home) = self.env.get_str("HOME", self) {
            Some(PathBuf::from(home.to_string()))
        } else {
            // HOME isn't set, so let's sort it out ourselves.
            users::get_current_user_home_dir()
        }
    }

    fn parser_options(&self) -> brush_parser::ParserOptions {
        brush_parser::ParserOptions {
            enable_extended_globbing: self.options.extended_globbing,
            posix_mode: self.options.posix_mode,
            sh_mode: self.options.sh_mode,
            tilde_expansion_at_word_start: true,
            tilde_expansion_after_colon: false,
            parser_impl: brush_parser::ParserImpl::Peg,
        }
    }

    fn apply_errexit_if_enabled(&self, result: &mut ExecutionResult) {
        if self.options.exit_on_nonzero_command_exit
            && !result.is_success()
            && result.is_normal_flow()
        {
            result.next_control_flow = ExecutionControlFlow::ExitShell;
        }
    }

    fn absolute_path(&self, path: &Path) -> PathBuf {
        if path.as_os_str().is_empty() || path.is_absolute() {
            path.to_owned()
        } else {
            self.working_dir().join(path)
        }
    }

    fn open_file(
        &self,
        options: &std::fs::OpenOptions,
        path: &Path,
        params: &ExecutionParameters,
    ) -> Result<openfiles::OpenFile, std::io::Error> {
        let path_to_open = self.absolute_path(path);

        // See if this is a reference to a file descriptor, in which case the actual
        // /dev/fd* file path for this process may not match with what's in the execution
        // parameters.
        if let Some(parent) = path_to_open.parent() {
            if parent == Path::new("/dev/fd") {
                if let Some(filename) = path_to_open.file_name() {
                    if let Ok(fd_num) = filename.to_string_lossy().to_string().parse::<ShellFd>() {
                        if let Some(open_file) = params.try_fd(self, fd_num) {
                            return open_file.try_clone();
                        }
                    }
                }
            }
        }

        Ok(options.open(path_to_open)?.into())
    }

    fn find_first_executable_in_path_using_cache(
        &mut self,
        candidate_name: &str,
    ) -> Option<PathBuf> {
        if let Some(cached_path) = self.program_location_cache.get(candidate_name) {
            Some(cached_path)
        } else if let Some(found_path) = self.find_first_executable_in_path(candidate_name) {
            self.program_location_cache
                .set(candidate_name, found_path.clone());
            Some(found_path)
        } else {
            None
        }
    }

    fn set_current_cmd(&mut self, cmd: &impl brush_parser::ast::Node) {
        self.call_stack
            .set_current_pos(cmd.location().map(|span| span.start));
    }

    fn enter_function(
        &mut self,
        name: &str,
        function: &functions::Registration,
        args: Vec<String>,
        _params: &ExecutionParameters,
    ) -> Result<(), error::Error> {
        if let Some(max_call_depth) = self.options.max_function_call_depth {
            if self.call_stack.function_call_depth() >= max_call_depth {
                return Err(error::ErrorKind::MaxFunctionCallDepthExceeded.into());
            }
        }

        if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
            let depth = self.call_stack.function_call_depth();
            let prefix = repeated_char_str(' ', depth);
            tracing::debug!(target: trace_categories::FUNCTIONS, "Entering func [depth={depth}]: {prefix}{name}");
        }

        self.call_stack.push_function(name, function, args);
        self.env.push_scope(env::EnvironmentScope::Local);

        Ok(())
    }

    fn leave_function(&mut self) -> Result<(), error::Error> {
        self.env.pop_scope(env::EnvironmentScope::Local)?;

        if let Some(exited_call) = self.call_stack.pop() {
            if let callstack::FrameType::Function(func_call) = exited_call.frame_type {
                if tracing::enabled!(target: trace_categories::FUNCTIONS, tracing::Level::DEBUG) {
                    let depth = self.call_stack.function_call_depth();
                    let prefix = repeated_char_str(' ', depth);
                    tracing::debug!(target: trace_categories::FUNCTIONS, "Exiting func  [depth={depth}]: {prefix}{}", func_call.function_name);
                }
            } else {
                let err: error::Error =
                    error::ErrorKind::InternalError("mismatched call stack state".to_owned())
                        .into();
                return Err(err.into_fatal());
            }
        }

        Ok(())
    }

    fn enter_trap_handler(&mut self, handler: Option<&traps::TrapHandler>) {
        self.call_stack.push_trap_handler(handler);
    }

    fn leave_trap_handler(&mut self) {
        self.call_stack.pop();
    }

    async fn trace_command(
        &mut self,
        params: &ExecutionParameters,
        command: &str,
    ) -> Result<(), error::Error> {
        // Expand the PS4 prompt variable to get our prefix.
        let ps4 = self.expand_prompt_var("PS4", "").await?;
        let mut prefix = ps4;

        // Add additional depth-based prefixes using the first character of PS4.
        let additional_depth = self.call_stack.script_source_depth() + self.depth;
        if let Some(c) = prefix.chars().next() {
            for _ in 0..additional_depth {
                prefix.insert(0, c);
            }
        }

        // Resolve which file descriptor to use for tracing. We default to stderr.
        let mut trace_file = params.try_stderr(self);

        // If BASH_XTRACEFD is set and refers to a valid file descriptor, use that instead.
        if let Some((_, xtracefd_var)) = self.env.get("BASH_XTRACEFD") {
            let xtracefd_value = xtracefd_var.value().to_cow_str(self);
            if let Ok(fd) = xtracefd_value.parse::<ShellFd>() {
                if let Some(file) = self.open_files.try_fd(fd) {
                    trace_file = Some(file.clone());
                }
            }
        }

        // If we have a valid trace file, write to it.
        if let Some(trace_file) = trace_file {
            let mut trace_file = trace_file.try_clone()?;
            writeln!(trace_file, "{prefix}{command}")?;
        }

        Ok(())
    }

    fn last_exit_status_change_count(&self) -> usize {
        self.last_exit_status_change_count
    }

    fn define_func_from_str(
        &mut self,
        name: impl Into<String>,
        body_text: &str,
    ) -> Result<(), error::Error> {
        let name = name.into();

        let mut parser = create_parser(body_text.as_bytes(), &self.parser_options());
        let func_body = parser.parse_function_parens_and_body().map_err(|e| {
            error::Error::from(error::ErrorKind::FunctionParseError(name.clone(), e))
        })?;

        let def = brush_parser::ast::FunctionDefinition {
            fname: name.clone().into(),
            body: func_body,
        };

        self.define_func(name, def, &crate::SourceInfo::default());

        Ok(())
    }

    fn define_func(
        &mut self,
        name: impl Into<String>,
        definition: brush_parser::ast::FunctionDefinition,
        source_info: &crate::SourceInfo,
    ) {
        let reg = functions::Registration::new(definition, source_info);
        self.funcs.update(name.into(), reg);
    }

    async fn invoke_function<
        N: AsRef<str> + Send,
        I: IntoIterator<Item = A> + Send,
        A: AsRef<str> + Send,
    >(
        &mut self,
        name: N,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<u8, error::Error> {
        let name = name.as_ref();
        let command_name = String::from(name);

        let func_registration = self
            .funcs
            .get(name)
            .ok_or_else(|| error::ErrorKind::FunctionNotFound(name.to_owned()))?
            .to_owned();

        let context = commands::ExecutionContext {
            shell: self,
            command_name,
            params: params.clone(),
        };

        let command_args = args
            .into_iter()
            .map(|s| commands::CommandArg::String(String::from(s.as_ref())))
            .collect::<Vec<_>>();

        let result =
            commands::invoke_shell_function(func_registration, context, &command_args).await?;

        match result.wait().await? {
            ExecutionWaitResult::Completed(result) => Ok(result.exit_code.into()),
            ExecutionWaitResult::Stopped(..) => {
                error::unimp("stopped child from function invocation")
            }
        }
    }

    async fn run_string<S: Into<String> + Send>(
        &mut self,
        command: S,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let parse_result = self.parse_string(command);
        self.run_parsed_result(parse_result, source_info, params)
            .await
    }

    fn parse_string<S: Into<String> + Send>(
        &self,
        s: S,
    ) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
        parse_string_impl(s.into(), self.parser_options())
    }

    async fn run_parsed_result(
        &mut self,
        parse_result: Result<brush_parser::ast::Program, brush_parser::ParseError>,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // If parsing succeeded, run the program. If there's a parse error, it's fatal (per spec).
        let result = match parse_result {
            Ok(prog) => self.run_program(prog, params).await,
            Err(parse_err) => Err(error::Error::from(error::ErrorKind::ParseError(
                parse_err,
                source_info.clone(),
            ))
            .into_fatal()),
        };

        // Report any errors.
        match result {
            Ok(result) => Ok(result),
            Err(err) => {
                let _ = self.display_error(&mut params.stderr(self), &err);

                let result = err.into_result(self);
                self.set_last_exit_status(result.exit_code.into());

                Ok(result)
            }
        }
    }

    fn tilde_shorten(&self, s: String) -> String {
        if let Some(home_dir) = self.home_dir() {
            if let Some(stripped) = s.strip_prefix(home_dir.to_string_lossy().as_ref()) {
                return format!("~{stripped}");
            }
        }
        s
    }

    fn display_error(
        &self,
        file: &mut (impl std::io::Write + Send),
        err: &error::Error,
    ) -> Result<(), error::Error> {
        let str = self.behavior.format_error(err, self);
        write!(file, "{str}")?;

        Ok(())
    }

    fn find_executables_in_path<'a>(
        &'a self,
        filename: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let paths = path_var.split(':').map(|s| s.to_owned());

        pathsearch::search_for_executable(paths.into_iter(), filename)
    }

    fn find_executables_in_path_with_prefix(
        &self,
        filename_prefix: &str,
        case_insensitive: bool,
    ) -> impl Iterator<Item = PathBuf> {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let paths = path_var.split(':').map(|s| s.to_owned());

        pathsearch::search_for_executable_with_prefix(
            paths.into_iter(),
            filename_prefix,
            case_insensitive,
        )
    }

    fn current_shell_args_mut(&mut self) -> &mut Vec<String> {
        for frame in self.call_stack.iter_mut() {
            match frame.frame_type {
                // Function calls always shadow positional parameters.
                callstack::FrameType::Function(..) => return &mut frame.args,
                // Executed scripts always shadow positional parameters.
                _ if frame.frame_type.is_run_script() => return &mut frame.args,
                // Sourced scripts shadow positional parameters if they have arguments.
                _ if frame.frame_type.is_sourced_script() => {
                    if !frame.args.is_empty() {
                        return &mut frame.args;
                    }
                }
                _ => (),
            }
        }

        &mut self.args
    }

    fn undefine_func(&mut self, name: &str) -> bool {
        self.funcs.remove(name).is_some()
    }

    async fn source_script<S: AsRef<str>, P: AsRef<Path> + Send, I: Iterator<Item = S> + Send>(
        &mut self,
        path: P,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        self.parse_and_execute_script_file(
            path.as_ref(),
            args,
            params,
            callstack::ScriptCallType::Source,
        )
        .await
    }

    async fn compose_precmd_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS0", "").await
    }

    async fn compose_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS1", self.default_prompt()).await
    }

    async fn compose_alt_side_prompt(&mut self) -> Result<String, error::Error> {
        // This is a brush extension.
        self.expand_prompt_var("BRUSH_PS_ALT", "").await
    }

    async fn compose_continuation_prompt(&mut self) -> Result<String, error::Error> {
        self.expand_prompt_var("PS2", "> ").await
    }

    fn in_sourced_script(&self) -> bool {
        self.call_stack.in_sourced_script()
    }

    fn in_function(&self) -> bool {
        self.call_stack.in_function()
    }

    fn history_file_path(&self) -> Option<PathBuf> {
        self.env_str("HISTFILE")
            .map(|s| PathBuf::from(s.into_owned()))
    }

    fn history_time_format(&self) -> Option<String> {
        self.env_str("HISTTIMEFORMAT").map(|s| s.into_owned())
    }

    fn add_to_history(&mut self, command: &str) -> Result<(), error::Error> {
        if let Some(history) = &mut self.history {
            // Trim.
            let command = command.trim();

            // For now, discard empty commands.
            if command.is_empty() {
                return Ok(());
            }

            // Add it to history.
            history.add(history::Item {
                id: 0,
                command_line: command.to_owned(),
                timestamp: Some(chrono::Utc::now()),
                dirty: true,
            })?;
        }

        Ok(())
    }

    fn builtin_mut(&mut self, name: &str) -> Option<&mut builtins::Registration<Self>> {
        self.builtins.get_mut(name)
    }

    async fn complete(
        &mut self,
        input: &str,
        position: usize,
    ) -> Result<completion::Completions, error::Error> {
        let completion_config = self.completion_config.clone();
        completion_config
            .get_completions(self, input, position)
            .await
    }

    fn set_working_dir(&mut self, target_dir: impl AsRef<Path>) -> Result<(), error::Error> {
        let abs_path = self.absolute_path(target_dir.as_ref());

        match std::fs::metadata(&abs_path) {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::ErrorKind::NotADirectory(abs_path).into());
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // Normalize the path (but don't canonicalize it).
        let cleaned_path = abs_path.normalize();

        let pwd = cleaned_path.to_string_lossy().to_string();

        self.env.update_or_add(
            "PWD",
            variables::ShellValueLiteral::Scalar(pwd),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;
        let oldpwd = std::mem::replace(&mut self.working_dir, cleaned_path);

        self.env.update_or_add(
            "OLDPWD",
            variables::ShellValueLiteral::Scalar(oldpwd.to_string_lossy().to_string()),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    fn replace_open_files(
        &mut self,
        open_fds: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) {
        self.open_files = openfiles::OpenFiles::from(open_fds);
    }

    fn is_keyword(&self, s: &str) -> bool {
        if self.options.sh_mode {
            keywords::SH_MODE_KEYWORDS.contains(s)
        } else {
            keywords::KEYWORDS.contains(s)
        }
    }

    fn check_for_completed_jobs(&mut self) -> Result<(), error::Error> {
        let results = self.jobs.poll()?;

        if self.options.enable_job_control {
            for (job, _result) in results {
                writeln!(self.stderr(), "{job}")?;
            }
        }

        Ok(())
    }

    fn eval_arithmetic(
        &mut self,
        expr: &brush_parser::ast::ArithmeticExpr,
    ) -> Result<i64, error::Error> {
        Ok(expr.eval(self)?)
    }

    fn func_mut(&mut self, name: &str) -> Option<&mut functions::Registration> {
        self.funcs.get_mut(name)
    }

    async fn basic_expand_string<S: AsRef<str> + Send>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<String, error::Error> {
        let result = expansion::basic_expand_word(self, params, s.as_ref()).await?;
        Ok(result)
    }

    #[cfg(feature = "serde")]
    fn as_serializable(&self) -> &impl serde::Serialize
    where
        Self: Sized,
    {
        self
    }

    fn increment_interactive_line_offset(&mut self, delta: usize) {
        self.call_stack.increment_current_line_offset(delta);
    }

    fn start_interactive_session(&mut self) -> Result<(), error::Error> {
        self.call_stack.push_interactive_session();
        Ok(())
    }

    fn end_interactive_session(&mut self) -> Result<(), error::Error> {
        if self
            .call_stack
            .current_frame()
            .is_none_or(|frame| !frame.frame_type.is_interactive_session())
        {
            return Err(error::ErrorKind::NotInInteractiveSession.into());
        }

        self.call_stack.pop();

        Ok(())
    }

    async fn on_exit(&mut self) -> Result<(), error::Error> {
        self.invoke_exit_trap_handler_if_registered().await?;

        Ok(())
    }

    fn save_history(&mut self) -> Result<(), error::Error> {
        if let Some(history_file_path) = self.history_file_path() {
            if let Some(history) = &mut self.history {
                // See if there's *any* time format configured. That triggers writing out
                // timestamps.
                let write_timestamps = self.env.is_set("HISTTIMEFORMAT");

                // TODO(history): Observe options.append_to_history_file
                history.flush(
                    history_file_path,
                    true, /* append? */
                    true, /* unsaved items only? */
                    write_timestamps,
                )?;
            }
        }

        Ok(())
    }

    fn stderr(&self) -> impl std::io::Write + Send + 'static {
        self.open_files.try_stderr().cloned().unwrap()
    }

    fn set_edit_buffer(&mut self, contents: String, cursor: usize) -> Result<(), error::Error> {
        self.env
            .set_global("READLINE_LINE", ShellVariable::new(contents))?;

        self.env
            .set_global("READLINE_POINT", ShellVariable::new(cursor.to_string()))?;

        Ok(())
    }

    fn pop_edit_buffer(&mut self) -> Result<Option<(String, usize)>, error::Error> {
        let line = self
            .env
            .unset("READLINE_LINE")?
            .map(|line| line.value().to_cow_str(self).to_string());

        let point = self
            .env
            .unset("READLINE_POINT")?
            .and_then(|point| point.value().to_cow_str(self).parse::<usize>().ok())
            .unwrap_or(0);

        if let Some(line) = line {
            Ok(Some((line, point)))
        } else {
            Ok(None)
        }
    }

    fn find_first_executable_in_path<S: AsRef<str>>(&self, candidate_name: S) -> Option<PathBuf> {
        for dir_str in self.env_str("PATH").unwrap_or_default().split(':') {
            let candidate_path = Path::new(dir_str).join(candidate_name.as_ref());
            if candidate_path.executable() {
                return Some(candidate_path);
            }
        }
        None
    }

    fn start_command_string_mode(&mut self) {
        self.call_stack.push_command_string();
    }

    fn end_command_string_mode(&mut self) -> Result<(), error::Error> {
        if self
            .call_stack
            .current_frame()
            .is_none_or(|frame| !frame.frame_type.is_command_string())
        {
            return Err(error::ErrorKind::NotExecutingCommandString.into());
        }

        self.call_stack.pop();

        Ok(())
    }

    async fn load_config(
        &mut self,
        profile_behavior: &ProfileLoadBehavior,
        rc_behavior: &RcLoadBehavior,
    ) -> Result<(), error::Error> {
        let mut params = ExecutionParameters::default();
        params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

        if self.options.login_shell {
            // --noprofile means skip this.
            if matches!(profile_behavior, ProfileLoadBehavior::Skip) {
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
            self.source_if_exists(Path::new("/etc/profile"), &params)
                .await?;
            if let Some(home_path) = self.home_dir() {
                if self.options.sh_mode {
                    self.source_if_exists(home_path.join(".profile").as_path(), &params)
                        .await?;
                } else {
                    if !self
                        .source_if_exists(home_path.join(".bash_profile").as_path(), &params)
                        .await?
                    {
                        if !self
                            .source_if_exists(home_path.join(".bash_login").as_path(), &params)
                            .await?
                        {
                            self.source_if_exists(home_path.join(".profile").as_path(), &params)
                                .await?;
                        }
                    }
                }
            }
        } else {
            if self.options.interactive {
                match rc_behavior {
                    _ if self.options.sh_mode => (),
                    RcLoadBehavior::Skip => (),
                    RcLoadBehavior::LoadCustom(rc_file) => {
                        // If an explicit rc file is provided, source it.
                        self.source_if_exists(rc_file, &params).await?;
                    }
                    RcLoadBehavior::LoadDefault => {
                        //
                        // Otherwise, for non-login interactive shells, load in this order:
                        //
                        //     /etc/bash.bashrc
                        //     ~/.bashrc
                        //
                        self.source_if_exists(Path::new("/etc/bash.bashrc"), &params)
                            .await?;
                        if let Some(home_path) = self.home_dir() {
                            self.source_if_exists(home_path.join(".bashrc").as_path(), &params)
                                .await?;
                            self.source_if_exists(home_path.join(".brushrc").as_path(), &params)
                                .await?;
                        }
                    }
                }
            } else {
                let env_var_name = if self.options.sh_mode {
                    "ENV"
                } else {
                    "BASH_ENV"
                };

                if self.env.is_set(env_var_name) {
                    //
                    // TODO(well-known-vars): look at $ENV/BASH_ENV; source its expansion if that
                    // file exists
                    //
                    return error::unimp(
                        "load config from $ENV/BASH_ENV for non-interactive, non-login shell",
                    );
                }
            }
        }

        Ok(())
    }

    async fn run_script<S: AsRef<str>, P: AsRef<Path> + Send, I: Iterator<Item = S> + Send>(
        &mut self,
        script_path: P,
        args: I,
    ) -> Result<ExecutionResult, error::Error> {
        let params = ExecutionParameters::default();
        let result = self
            .parse_and_execute_script_file(
                script_path.as_ref(),
                args,
                &params,
                callstack::ScriptCallType::Run,
            )
            .await?;

        let _ = self.on_exit().await;

        Ok(result)
    }
}

impl<SB: ShellBehavior> Default for Shell<SB> {
    fn default() -> Self {
        Self {
            behavior: SB::default(),
            traps: traps::TrapHandlerConfig::default(),
            open_files: openfiles::OpenFiles::default(),
            working_dir: PathBuf::default(),
            env: env::ShellEnvironment::default(),
            funcs: functions::FunctionEnv::default(),
            options: RuntimeOptions::default(),
            jobs: jobs::JobManager::default(),
            aliases: HashMap::default(),
            last_exit_status: 0,
            last_exit_status_change_count: 0,
            last_pipeline_statuses: vec![0],
            depth: 0,
            name: None,
            args: vec![],
            version: None,
            product_display_str: None,
            call_stack: callstack::CallStack::new(),
            directory_stack: vec![],
            completion_config: completion::Config::default(),
            builtins: HashMap::default(),
            program_location_cache: pathcache::PathCache::default(),
            last_stopwatch_time: std::time::SystemTime::now(),
            last_stopwatch_offset: 0,
            key_bindings: None,
            history: None,
        }
    }
}

impl<SB: ShellBehavior> Shell<SB> {
    /// Returns a new shell instance created with the given options.
    /// Does *not* load any configuration files (e.g., bashrc).
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use when creating the shell.
    pub(crate) fn new(options: CreateOptions<SB>) -> Result<Self, error::Error> {
        // Instantiate the shell with some defaults.
        let mut shell = Self {
            open_files: openfiles::OpenFiles::new(),
            options: RuntimeOptions::defaults_from(&options),
            name: options.shell_name,
            args: options.shell_args.unwrap_or_default(),
            version: options.shell_version,
            product_display_str: options.shell_product_display_str,
            working_dir: options.working_dir.map_or_else(std::env::current_dir, Ok)?,
            builtins: options.builtins,
            key_bindings: options.key_bindings,
            ..Self::default()
        };

        // Add in any open files provided.
        shell.open_files.update_from(options.fds.into_iter());

        // TODO(patterns): Without this a script that sets extglob will fail because we
        // parse the entire script with the same settings.
        shell.options.extended_globbing = true;

        // If requested, seed parameters from environment.
        if !options.do_not_inherit_env {
            wellknownvars::inherit_env_vars(&mut shell)?;
        }

        // If requested, set well-known variables.
        if !options.skip_well_known_vars {
            wellknownvars::init_well_known_vars(&mut shell)?;
        }

        // Set any provided variables.
        for (var_name, var_value) in options.vars {
            shell.env.set_global(var_name, var_value)?;
        }

        // Set up history, if relevant. Do NOT fail if we can't load history.
        if shell.options.enable_command_history {
            shell.history = shell
                .load_history()
                .unwrap_or_default()
                .or_else(|| Some(history::History::default()));
        }

        Ok(shell)
    }

    fn load_history(&self) -> Result<Option<history::History>, error::Error> {
        const MAX_FILE_SIZE_FOR_HISTORY_IMPORT: u64 = 1024 * 1024 * 1024; // 1 GiB

        let Some(history_path) = self.history_file_path() else {
            return Ok(None);
        };

        let mut options = std::fs::File::options();
        options.read(true);

        let mut history_file =
            self.open_file(&options, &history_path, &ExecutionParameters::default())?;

        // Check on the file's size.
        if let openfiles::OpenFile::File(file) = &mut history_file {
            let file_metadata = file.metadata()?;
            let file_size = file_metadata.len();

            // If the file is empty, no reason to try reading it. Note that this will also
            // end up excluding non-regular files that report a 0 file size but appear
            // to have contents when read.
            if file_size == 0 {
                return Ok(None);
            }

            // Bail if the file is unrealistically large. For now we just refuse to import it.
            if file_size > MAX_FILE_SIZE_FOR_HISTORY_IMPORT {
                return Err(error::ErrorKind::HistoryFileTooLargeToImport.into());
            }
        }

        Ok(Some(history::History::import(history_file)?))
    }

    async fn source_if_exists(
        &mut self,
        path: impl AsRef<Path>,
        params: &ExecutionParameters,
    ) -> Result<bool, error::Error> {
        let path = path.as_ref();
        if path.exists() {
            self.source_script(path, std::iter::empty::<String>(), params)
                .await?;
            Ok(true)
        } else {
            tracing::debug!("skipping non-existent file: {}", path.display());
            Ok(false)
        }
    }

    /// Parse and execute the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn parse_and_execute_script_file<S: AsRef<str>, P: AsRef<Path>, I: Iterator<Item = S>>(
        &mut self,
        path: P,
        args: I,
        params: &ExecutionParameters,
        call_type: callstack::ScriptCallType,
    ) -> Result<ExecutionResult, error::Error> {
        let path = path.as_ref();
        tracing::debug!("sourcing: {}", path.display());

        let mut options = std::fs::File::options();
        options.read(true);

        let opened_file: openfiles::OpenFile = self
            .open_file(&options, path, params)
            .map_err(|e| error::ErrorKind::FailedSourcingFile(path.to_owned(), e))?;

        if opened_file.is_dir() {
            return Err(error::ErrorKind::FailedSourcingFile(
                path.to_owned(),
                std::io::Error::from(std::io::ErrorKind::IsADirectory),
            )
            .into());
        }

        let source_info = crate::SourceInfo::from(path.to_owned());

        let mut result = self
            .source_file(opened_file, &source_info, args, params, call_type)
            .await?;

        // Handle control flow at script execution boundary. If execution completed
        // with a `return`, we need to clear it since it's already been "used". All
        // other control flow types are preserved.
        if matches!(
            result.next_control_flow,
            ExecutionControlFlow::ReturnFromFunctionOrScript
        ) {
            result.next_control_flow = ExecutionControlFlow::Normal;
        }

        Ok(result)
    }

    /// Source the given file as a shell script, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `file` - The file to source.
    /// * `source_info` - Information about the source of the script.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    /// * `call_type` - The type of script call being made.
    async fn source_file<F: Read, S: AsRef<str>, I: Iterator<Item = S>>(
        &mut self,
        file: F,
        source_info: &crate::SourceInfo,
        args: I,
        params: &ExecutionParameters,
        call_type: callstack::ScriptCallType,
    ) -> Result<ExecutionResult, error::Error> {
        let mut reader = std::io::BufReader::new(file);
        let mut parser = brush_parser::Parser::new(&mut reader, &self.parser_options());

        tracing::debug!(target: trace_categories::PARSE, "Parsing sourced file: {}", source_info.source);
        let parse_result = parser.parse_program();

        let script_positional_args = args.map(|s| s.as_ref().to_owned());

        self.call_stack
            .push_script(call_type, source_info, script_positional_args);

        let result = self
            .run_parsed_result(parse_result, source_info, params)
            .await;

        self.call_stack.pop();

        result
    }

    /// Parses the given reader as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    pub fn parse<R: Read>(
        &self,
        reader: R,
    ) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
        let mut parser = create_parser(reader, &self.parser_options());

        tracing::debug!(target: trace_categories::PARSE, "Parsing reader as program...");
        parser.parse_program()
    }

    /// Applies full shell expansion and field splitting to the provided string; returns
    /// a sequence of fields.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand and split.
    pub async fn full_expand_and_split_string<S: AsRef<str>>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<Vec<String>, error::Error> {
        let result = expansion::full_expand_and_split_word(self, params, s.as_ref()).await?;
        Ok(result)
    }

    async fn invoke_exit_trap_handler_if_registered(
        &mut self,
    ) -> Result<ExecutionResult, error::Error> {
        let Some(handler) = self.traps.get_handler(traps::TrapSignal::Exit).cloned() else {
            return Ok(ExecutionResult::success());
        };

        // TODO(traps): Confirm whether trap handlers should be executed in the same process group.
        let mut params = ExecutionParameters::default();
        params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

        let orig_last_exit_status = self.last_exit_status;

        self.enter_trap_handler(Some(&handler));

        let result = self
            .run_string(&handler.command, &handler.source_info, &params)
            .await;

        self.leave_trap_handler();
        self.last_exit_status = orig_last_exit_status;

        result
    }

    /// Executes the given parsed shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `program` - The program to execute.
    /// * `params` - Execution parameters.
    pub async fn run_program(
        &mut self,
        program: brush_parser::ast::Program,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        program.execute(self, params).await
    }

    const fn default_prompt(&self) -> &'static str {
        if self.options.sh_mode {
            "$ "
        } else {
            "brush$ "
        }
    }

    async fn expand_prompt_var(
        &mut self,
        var_name: &str,
        default: &str,
    ) -> Result<String, error::Error> {
        //
        // TODO(prompt): bash appears to do this in a subshell; we need to investigate
        // if that's required.
        //

        // Retrieve the spec.
        let prompt_spec = self.parameter_or_default(var_name, default);
        if prompt_spec.is_empty() {
            return Ok(String::new());
        }

        // Save (and later restore) the last exit status.
        let prev_last_result = self.last_exit_status();
        let prev_last_pipeline_statuses = self.last_pipeline_statuses.clone();

        // Expand it.
        let params = ExecutionParameters::default();
        let result = prompt::expand_prompt(self, &params, prompt_spec.into_owned()).await;

        // Restore the last exit status.
        self.last_pipeline_statuses = prev_last_pipeline_statuses;
        self.set_last_exit_status(prev_last_result);

        result
    }

    fn parameter_or_default<'a>(&'a self, name: &str, default: &'a str) -> Cow<'a, str> {
        self.env_str(name).unwrap_or_else(|| default.into())
    }

    /// Tries to set a global variable in the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to add.
    /// * `var` - The variable contents to add.
    pub fn set_env_global(&mut self, name: &str, var: ShellVariable) -> Result<(), error::Error> {
        self.env.set_global(name, var)
    }

    /// Register a builtin to the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The in-shell name of the builtin.
    /// * `registration` - The registration handle for the builtin.
    pub fn register_builtin<S: Into<String>>(
        &mut self,
        name: S,
        registration: builtins::Registration<Self>,
    ) {
        self.builtins.insert(name.into(), registration);
    }

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard output stream using `write!` at al.
    pub fn stdout(&self) -> impl std::io::Write + 'static {
        self.open_files.try_stdout().cloned().unwrap()
    }
}

#[cached::proc_macro::cached(size = 64, result = true)]
fn parse_string_impl(
    s: String,
    parser_options: brush_parser::ParserOptions,
) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
    let mut parser = create_parser(s.as_bytes(), &parser_options);

    tracing::debug!(target: trace_categories::PARSE, "Parsing string as program...");
    parser.parse_program()
}

fn create_parser<R: Read>(
    r: R,
    parser_options: &brush_parser::ParserOptions,
) -> brush_parser::Parser<std::io::BufReader<R>> {
    let reader = std::io::BufReader::new(r);
    brush_parser::Parser::new(reader, parser_options)
}

fn repeated_char_str(c: char, count: usize) -> String {
    (0..count).map(|_| c).collect()
}

#[cfg(feature = "serde")]
fn default_shell_behavior<SB: ShellBehavior>() -> SB {
    SB::default()
}
