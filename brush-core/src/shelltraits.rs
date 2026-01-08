//! Definition of key shell traits (no concrete implementations).

use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    ExecutionParameters, ExecutionResult, ProfileLoadBehavior, RcLoadBehavior, ShellFd,
    ShellVariable, builtins, callstack, completion, env::ShellEnvironment, error, functions,
    history, jobs, openfiles, options::RuntimeOptions, pathcache, shell::KeyBindingsHelper, traps,
};

/// Trait implemented by shell runtimes.
///
/// This trait provides access to the core shell state including variables,
/// options, builtins, jobs, traps, and other shell infrastructure.
#[async_trait::async_trait]
pub trait ShellRuntime: Send + Sync + 'static {
    /// Clones the shell runtime, yielding a subshell instance.
    #[must_use]
    fn clone_subshell(&self) -> Self
    where
        Self: Sized;

    /// Returns the shell's official version string (if available).
    fn version(&self) -> &Option<String>;

    /// Returns the call stack for the shell.
    fn call_stack(&self) -> &callstack::CallStack;

    /// Returns the exit status of the last command executed in this shell.
    fn last_exit_status(&self) -> u8;

    /// Updates the last exit status.
    fn set_last_exit_status(&mut self, status: u8);

    /// Returns the key bindings helper for the shell.
    fn key_bindings(&self) -> &Option<KeyBindingsHelper>;

    /// Sets the key bindings helper for the shell.
    fn set_key_bindings(&mut self, key_bindings: Option<KeyBindingsHelper>);

    /// Returns the registered builtins for the shell.
    fn builtins(&self) -> &HashMap<String, builtins::Registration<Self>>
    where
        Self: Sized;

    /// Returns the shell's current working directory.
    fn working_dir(&self) -> &Path;

    /// Returns the product display name for this shell.
    fn product_display_str(&self) -> &Option<String>;

    /// Returns the function definition environment for this shell.
    fn funcs(&self) -> &functions::FunctionEnv;

    /// Returns a mutable reference to the function definition environment for this shell.
    fn funcs_mut(&mut self) -> &mut functions::FunctionEnv;

    /// Returns the shell environment containing variables.
    fn env(&self) -> &ShellEnvironment;

    /// Returns a mutable reference to the shell environment.
    fn env_mut(&mut self) -> &mut ShellEnvironment;

    /// Returns the shell's runtime options.
    fn options(&self) -> &RuntimeOptions;

    /// Returns a mutable reference to the shell's runtime options.
    fn options_mut(&mut self) -> &mut RuntimeOptions;

    /// Returns the shell's aliases.
    fn aliases(&self) -> &HashMap<String, String>;

    /// Returns a mutable reference to the shell's aliases.
    fn aliases_mut(&mut self) -> &mut HashMap<String, String>;

    /// Returns the shell's job manager.
    fn jobs(&self) -> &jobs::JobManager;

    /// Returns a mutable reference to the shell's job manager.
    fn jobs_mut(&mut self) -> &mut jobs::JobManager;

    /// Returns the shell's trap handler configuration.
    fn traps(&self) -> &traps::TrapHandlerConfig;

    /// Returns a mutable reference to the shell's trap handler configuration.
    fn traps_mut(&mut self) -> &mut traps::TrapHandlerConfig;

    /// Returns the shell's directory stack.
    fn directory_stack(&self) -> &[PathBuf];

    /// Returns a mutable reference to the shell's directory stack.
    fn directory_stack_mut(&mut self) -> &mut Vec<PathBuf>;

    /// Returns the statuses of commands in the last pipeline.
    fn last_pipeline_statuses(&self) -> &[u8];

    /// Returns a mutable reference to the statuses of commands in the last pipeline.
    fn last_pipeline_statuses_mut(&mut self) -> &mut Vec<u8>;

    /// Returns the shell's program location cache.
    fn program_location_cache(&self) -> &pathcache::PathCache;

    /// Returns a mutable reference to the shell's program location cache.
    fn program_location_cache_mut(&mut self) -> &mut pathcache::PathCache;

    /// Returns the shell's completion configuration.
    fn completion_config(&self) -> &completion::Config;

    /// Returns a mutable reference to the shell's completion configuration.
    fn completion_config_mut(&mut self) -> &mut completion::Config;

    /// Returns the shell's open files.
    fn open_files(&self) -> &openfiles::OpenFiles;

    /// Returns a mutable reference to the shell's open files.
    fn open_files_mut(&mut self) -> &mut openfiles::OpenFiles;

    /// Returns the last "SECONDS" captured time.
    fn last_stopwatch_time(&self) -> std::time::SystemTime;

    /// Returns the last "SECONDS" offset requested.
    fn last_stopwatch_offset(&self) -> u32;

    /// Returns the shell's history, if it exists.
    fn history(&self) -> Option<&history::History>;

    /// Returns a mutable reference to the shell's history, if it exists.
    fn history_mut(&mut self) -> Option<&mut history::History>;

    /// Returns whether or not this shell is a subshell.
    fn is_subshell(&self) -> bool;

    /// Returns the current subshell depth; 0 is returned if this shell is not a subshell.
    fn depth(&self) -> usize;

    /// Tries to retrieve a variable from the shell's environment, converting it into its
    /// string form.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    fn env_str(&self, name: &str) -> Option<Cow<'_, str>>;

    /// Tries to retrieve a variable from the shell's environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    fn env_var(&self, name: &str) -> Option<&ShellVariable>;

    /// Returns the *current* name of the shell ($0).
    /// Influenced by the current call stack.
    fn current_shell_name(&self) -> Option<Cow<'_, str>>;

    /// Returns the *current* positional arguments for the shell ($1 and beyond).
    /// Influenced by the current call stack.
    fn current_shell_args(&self) -> &[String];

    /// Returns the current value of the IFS variable, or the default value if it is not set.
    fn ifs(&self) -> Cow<'_, str>;

    /// Returns the first character of the IFS variable, or a space if it is not set.
    fn get_ifs_first_char(&self) -> char;

    /// Returns the keywords that are reserved by the shell.
    fn get_keywords(&self) -> Vec<String>;

    /// Returns the shell's current home directory, if available.
    fn home_dir(&self) -> Option<PathBuf>;

    /// Returns the options that should be used for parsing shell programs; reflects
    /// the current configuration state of the shell and may change over time.
    fn parser_options(&self) -> brush_parser::ParserOptions;

    /// Applies errexit semantics to a result if enabled and appropriate.
    /// This should be called at "statement boundaries" where errexit should be checked.
    ///
    /// # Arguments
    ///
    /// * `result` - The execution result to potentially modify.
    fn apply_errexit_if_enabled(&self, result: &mut ExecutionResult);

    /// Gets the absolute form of the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to get the absolute form of.
    fn absolute_path(&self, path: &Path) -> PathBuf;

    /// Opens the given file, using the context of this shell and the provided execution parameters.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use opening the file.
    /// * `path` - The path to the file to open; may be relative to the shell's working directory.
    /// * `params` - Execution parameters.
    fn open_file(
        &self,
        options: &std::fs::OpenOptions,
        path: &Path,
        params: &ExecutionParameters,
    ) -> Result<openfiles::OpenFile, std::io::Error>;

    /// Uses the shell's hash-based path cache to check whether the given filename is the name
    /// of an executable in one of the directories in the shell's current PATH. If found,
    /// ensures the path is in the cache and returns it.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    fn find_first_executable_in_path_using_cache(
        &mut self,
        candidate_name: &str,
    ) -> Option<PathBuf>;

    /// Updates the currently executing command in the shell.
    fn set_current_cmd(&mut self, cmd: &impl brush_parser::ast::Node)
    where
        Self: Sized;

    /// Updates the shell's internal tracking state to reflect that a new shell
    /// function is being entered.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function being entered.
    /// * `function` - The function being entered.
    /// * `args` - The arguments being passed to the function.
    /// * `params` - Current execution parameters.
    fn enter_function(
        &mut self,
        name: &str,
        function: &functions::Registration,
        args: Vec<String>,
        params: &ExecutionParameters,
    ) -> Result<(), error::Error>;

    /// Updates the shell's internal tracking state to reflect that the shell
    /// has exited the top-most function on its call stack.
    fn leave_function(&mut self) -> Result<(), error::Error>;

    /// Updates the shell's internal tracking state to reflect that we're entering
    /// a trap handler.
    fn enter_trap_handler(&mut self, handler: Option<&traps::TrapHandler>);

    /// Updates the shell's internal tracking state to reflect that we're leaving
    /// a trap handler.
    fn leave_trap_handler(&mut self);

    /// Outputs `set -x` style trace output for a command.
    ///
    /// # Arguments
    ///
    /// * `params` - The execution parameters.
    /// * `command` - The command to trace.
    async fn trace_command(
        &mut self,
        params: &ExecutionParameters,
        command: &str,
    ) -> Result<(), error::Error>;

    /// Returns the count of how many times the last exit status has changed.
    fn last_exit_status_change_count(&self) -> usize;

    /// Tries to define a function in the shell's environment using the given
    /// string as its body.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function
    /// * `body_text` - The body of the function, expected to start with "()".
    fn define_func_from_str(
        &mut self,
        name: impl Into<String>,
        body_text: &str,
    ) -> Result<(), error::Error>
    where
        Self: Sized;

    /// Defines a function in the shell's environment. If a function already exists
    /// with the given name, it is replaced with the new definition.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to define.
    /// * `definition` - The function's definition.
    /// * `source_info` - Source information for the function definition.
    fn define_func(
        &mut self,
        name: impl Into<String>,
        definition: brush_parser::ast::FunctionDefinition,
        source_info: &crate::SourceInfo,
    ) where
        Self: Sized;

    /// Invokes the named function, passing the given arguments, and returns the
    /// exit status of the function.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to invoke.
    /// * `args` - The arguments to pass to the function.
    /// * `params` - Execution parameters to use for the invocation.
    async fn invoke_function<
        N: AsRef<str> + Send,
        I: IntoIterator<Item = A> + Send,
        A: AsRef<str> + Send,
    >(
        &mut self,
        name: N,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<u8, error::Error>
    where
        Self: Sized;

    /// Executes the given string as a shell program, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to execute.
    /// * `source_info` - Information about the source of the command text.
    /// * `params` - Execution parameters.
    async fn run_string<S: Into<String> + Send>(
        &mut self,
        command: S,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error>
    where
        Self: Sized;

    /// Parses the given string as a shell program, returning the resulting Abstract Syntax Tree
    /// for the program.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to parse as a program.
    fn parse_string<S: Into<String> + Send>(
        &self,
        s: S,
    ) -> Result<brush_parser::ast::Program, brush_parser::ParseError>
    where
        Self: Sized;

    /// Executes a parsed shell program result, handling parse errors appropriately.
    ///
    /// # Arguments
    ///
    /// * `parse_result` - The result of parsing a shell program.
    /// * `source_info` - Information about the source of the command text.
    /// * `params` - Execution parameters.
    async fn run_parsed_result(
        &mut self,
        parse_result: Result<brush_parser::ast::Program, brush_parser::ParseError>,
        source_info: &crate::SourceInfo,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error>;

    /// Tilde-shortens the given string, replacing the user's home directory with a tilde.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to shorten.
    fn tilde_shorten(&self, s: String) -> String;

    /// Displays the given error to the user, using the shell's error display mechanisms.
    ///
    /// # Arguments
    ///
    /// * `file` - The file to write the error to.
    /// * `err` - The error to display.
    fn display_error(
        &self,
        file: &mut (impl std::io::Write + Send),
        err: &error::Error,
    ) -> Result<(), error::Error>
    where
        Self: Sized;

    /// Finds executables in the shell's current default PATH, matching the given filename.
    ///
    /// # Arguments
    ///
    /// * `filename` - The filename to match against.
    fn find_executables_in_path<'a>(
        &'a self,
        filename: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a
    where
        Self: Sized;

    /// Finds executables in the shell's current default PATH, with filenames matching the
    /// given prefix.
    ///
    /// # Arguments
    ///
    /// * `filename_prefix` - The prefix to match against executable filenames.
    fn find_executables_in_path_with_prefix(
        &self,
        filename_prefix: &str,
        case_insensitive: bool,
    ) -> impl Iterator<Item = PathBuf>
    where
        Self: Sized;

    /// Returns a mutable reference to *current* positional parameters for the shell
    /// ($1 and beyond).
    fn current_shell_args_mut(&mut self) -> &mut Vec<String>;

    /// Tries to undefine a function in the shell's environment. Returns whether or
    /// not a definition was removed.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to undefine.
    fn undefine_func(&mut self, name: &str) -> bool;

    /// Sources a script file, returning the execution result.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file to source.
    /// * `args` - The arguments to pass to the script as positional parameters.
    /// * `params` - Execution parameters.
    async fn source_script<S: AsRef<str>, P: AsRef<Path> + Send, I: Iterator<Item = S> + Send>(
        &mut self,
        path: P,
        args: I,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error>
    where
        Self: Sized;

    /// Composes the shell's post-input, pre-command prompt, applying all appropriate expansions.
    async fn compose_precmd_prompt(&mut self) -> Result<String, error::Error>
    where
        Self: Sized;

    /// Composes the shell's prompt, applying all appropriate expansions.
    async fn compose_prompt(&mut self) -> Result<String, error::Error>
    where
        Self: Sized;

    /// Compose's the shell's alternate-side prompt, applying all appropriate expansions.
    async fn compose_alt_side_prompt(&mut self) -> Result<String, error::Error>
    where
        Self: Sized;

    /// Composes the shell's continuation prompt.
    async fn compose_continuation_prompt(&mut self) -> Result<String, error::Error>
    where
        Self: Sized;

    /// Returns whether or not the shell is actively executing in a sourced script.
    fn in_sourced_script(&self) -> bool;

    /// Returns whether or not the shell is actively executing in a shell function.
    fn in_function(&self) -> bool;

    /// Returns the path to the history file used by the shell, if one is set.
    fn history_file_path(&self) -> Option<PathBuf>;

    /// Returns the path to the history file used by the shell, if one is set.
    fn history_time_format(&self) -> Option<String>;

    /// Adds a command to history.
    fn add_to_history(&mut self, command: &str) -> Result<(), error::Error>;

    /// Tries to retrieve a mutable reference to an existing builtin registration.
    /// Returns `None` if no such registration exists.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the builtin to lookup.
    fn builtin_mut(&mut self, name: &str) -> Option<&mut builtins::Registration<Self>>
    where
        Self: Sized;

    /// Generates command completions for the shell.
    ///
    /// # Arguments
    ///
    /// * `input` - The input string to generate completions for.
    /// * `position` - The position in the input string to generate completions at.
    async fn complete(
        &mut self,
        input: &str,
        position: usize,
    ) -> Result<completion::Completions, error::Error>
    where
        Self: Sized;

    /// Sets the shell's current working directory to the given path.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The path to set as the working directory.
    fn set_working_dir(&mut self, target_dir: impl AsRef<Path>) -> Result<(), error::Error>
    where
        Self: Sized;

    /// Replaces the shell's currently configured open files with the given set.
    /// Typically only used by exec-like builtins.
    ///
    /// # Arguments
    ///
    /// * `open_files` - The new set of open files to use.
    fn replace_open_files(
        &mut self,
        open_fds: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) where
        Self: Sized;

    /// Checks if the given string is a keyword reserved in this shell.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to check.
    fn is_keyword(&self, s: &str) -> bool;

    /// Checks for completed jobs in the shell, reporting any changes found.
    fn check_for_completed_jobs(&mut self) -> Result<(), error::Error>;

    /// Evaluate the given arithmetic expression, returning the result.
    fn eval_arithmetic(
        &mut self,
        expr: &brush_parser::ast::ArithmeticExpr,
    ) -> Result<i64, error::Error>;

    /// Tries to retrieve a mutable reference to an existing function registration.
    /// Returns `None` if no such registration exists.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the function to lookup.
    fn func_mut(&mut self, name: &str) -> Option<&mut functions::Registration>;

    /// Applies basic shell expansion to the provided string.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to expand.
    async fn basic_expand_string<S: AsRef<str> + Send>(
        &mut self,
        params: &ExecutionParameters,
        s: S,
    ) -> Result<String, error::Error>
    where
        Self: Sized;

    /// Returns a serializable representation of the shell.
    #[cfg(feature = "serde")]
    fn as_serializable(&self) -> &impl serde::Serialize
    where
        Self: Sized;

    /// Increments the interactive line offset in the shell by the indicated number
    /// of lines.
    ///
    /// # Arguments
    ///
    /// * `delta` - The number of lines to increment the current line offset by.
    fn increment_interactive_line_offset(&mut self, delta: usize);

    /// Updates the shell's internal tracking state to reflect that a new interactive
    /// session is being started.
    fn start_interactive_session(&mut self) -> Result<(), error::Error>;

    /// Updates the shell's internal tracking state to reflect that the current
    /// interactive session is ending.
    fn end_interactive_session(&mut self) -> Result<(), error::Error>;

    /// Runs any exit steps for the shell.
    async fn on_exit(&mut self) -> Result<(), error::Error>;

    /// Saves history back to any backing storage.
    fn save_history(&mut self) -> Result<(), error::Error>;

    /// Returns a value that can be used to write to the shell's currently configured
    /// standard error stream using `write!` et al.
    fn stderr(&self) -> impl std::io::Write + Send + 'static
    where
        Self: Sized;

    /// Updates the shell state to reflect the given edit buffer contents.
    ///
    /// # Arguments
    ///
    /// * `contents` - The contents of the edit buffer.
    /// * `cursor` - The cursor position in the edit buffer.
    fn set_edit_buffer(&mut self, contents: String, cursor: usize) -> Result<(), error::Error>;

    /// Returns the contents of the shell's edit buffer, if any. The buffer
    /// state is cleared from the shell.
    fn pop_edit_buffer(&mut self) -> Result<Option<(String, usize)>, error::Error>;

    /// Determines whether the given filename is the name of an executable in one of the
    /// directories in the shell's current PATH. If found, returns the path.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    fn find_first_executable_in_path<S: AsRef<str>>(&self, candidate_name: S) -> Option<PathBuf>
    where
        Self: Sized;

    /// Updates the shell's internal tracking state to reflect that command
    /// string mode is being started.
    fn start_command_string_mode(&mut self);

    /// Updates the shell's internal tracking state to reflect that command
    /// string mode is ending.
    fn end_command_string_mode(&mut self) -> Result<(), error::Error>;

    /// Loads and executes standard shell configuration files (i.e., rc and profile).
    ///
    /// # Arguments
    ///
    /// * `profile_behavior` - Behavior for loading profile files.
    /// * `rc_behavior` - Behavior for loading rc files.
    async fn load_config(
        &mut self,
        profile_behavior: &ProfileLoadBehavior,
        rc_behavior: &RcLoadBehavior,
    ) -> Result<(), error::Error>;

    /// Executes the given script file, returning the resulting exit status.
    ///
    /// # Arguments
    ///
    /// * `script_path` - The path to the script file to execute.
    /// * `args` - The arguments to pass to the script as positional parameters.
    async fn run_script<S: AsRef<str>, P: AsRef<Path> + Send, I: Iterator<Item = S> + Send>(
        &mut self,
        script_path: P,
        args: I,
    ) -> Result<ExecutionResult, error::Error>
    where
        Self: Sized;
}
