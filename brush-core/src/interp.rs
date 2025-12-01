use brush_parser::ast::{self, CommandPrefixOrSuffixItem};
use itertools::Itertools;
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::arithmetic::{self, ExpandAndEvaluate};
use crate::commands::{self, CommandArg};
use crate::env::{EnvironmentLookup, EnvironmentScope};
use crate::openfiles::{OpenFile, OpenFiles};
use crate::results::{
    ExecutionExitCode, ExecutionResult, ExecutionSpawnResult, ExecutionWaitResult,
};
use crate::shell::Shell;
use crate::variables::{
    ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
};
use crate::{ShellFd, error, expansion, extendedtests, jobs, openfiles, sys, timing};

/// Encapsulates the context of execution in a command pipeline.
struct PipelineExecutionContext<'a> {
    /// The shell in which the command should be executed.
    shell: commands::ShellForCommand<'a>,
    /// Process group ID for spawned processes.
    process_group_id: Option<i32>,
}

/// Parameters for execution.
#[derive(Clone, Default)]
pub struct ExecutionParameters {
    /// The open files tracked by the current context.
    open_files: openfiles::OpenFiles,
    /// Policy for how to manage spawned external processes.
    pub process_group_policy: ProcessGroupPolicy,
}

impl ExecutionParameters {
    /// Returns the standard input file; usable with `write!` et al.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn stdin(&self, shell: &Shell) -> impl std::io::Read + 'static {
        self.try_stdin(shell).unwrap()
    }

    /// Tries to retrieve the standard input file. Returns `None` if not set.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn try_stdin(&self, shell: &Shell) -> Option<OpenFile> {
        self.try_fd(shell, openfiles::OpenFiles::STDIN_FD)
    }

    /// Returns the standard output file; usable with `write!` et al.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn stdout(&self, shell: &Shell) -> impl std::io::Write + 'static {
        self.try_stdout(shell).unwrap()
    }

    /// Tries to retrieve the standard output file. Returns `None` if not set.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn try_stdout(&self, shell: &Shell) -> Option<OpenFile> {
        self.try_fd(shell, openfiles::OpenFiles::STDOUT_FD)
    }

    /// Returns the standard error file; usable with `write!` et al.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn stderr(&self, shell: &Shell) -> impl std::io::Write + 'static {
        self.try_stderr(shell).unwrap()
    }

    /// Tries to retrieve the standard error file. Returns `None` if not set.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn try_stderr(&self, shell: &Shell) -> Option<OpenFile> {
        self.try_fd(shell, openfiles::OpenFiles::STDERR_FD)
    }

    /// Returns the file descriptor with the given number. Returns `None`
    /// if the file descriptor is not open.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    /// * `fd` - The file descriptor number to retrieve.
    pub fn try_fd(&self, shell: &Shell, fd: ShellFd) -> Option<openfiles::OpenFile> {
        match self.open_files.fd_entry(fd) {
            openfiles::OpenFileEntry::Open(f) => Some(f.clone()),
            openfiles::OpenFileEntry::NotPresent => None,
            openfiles::OpenFileEntry::NotSpecified => {
                // We didn't have this fd specified one way or the other; we fallback
                // to what's represented in the shell's open files.
                shell.persistent_open_files().try_fd(fd).cloned()
            }
        }
    }

    /// Sets the given file descriptor to the provided open file.
    ///
    /// # Arguments
    ///
    /// * `fd` - The file descriptor number to set.
    /// * `file` - The open file to set.
    pub fn set_fd(&mut self, fd: ShellFd, file: openfiles::OpenFile) {
        self.open_files.set_fd(fd, file);
    }

    /// Iterates over all open file descriptors in this context.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell context.
    pub fn iter_fds(&self, shell: &Shell) -> impl Iterator<Item = (ShellFd, openfiles::OpenFile)> {
        let our_fds = self.open_files.iter_fds();
        let shell_fds = shell
            .persistent_open_files()
            .iter_fds()
            .filter(|(fd, _)| !self.open_files.contains_fd(*fd));

        #[allow(clippy::needless_collect)]
        let all_fds: Vec<_> = our_fds
            .chain(shell_fds)
            .map(|(fd, file)| (fd, file.clone()))
            .collect();

        all_fds.into_iter()
    }
}

#[derive(Clone, Debug, Default)]
/// Policy for how to manage spawned external processes.
pub enum ProcessGroupPolicy {
    /// Place the process in a new process group.
    #[default]
    NewProcessGroup,
    /// Place the process in the same process group as its parent.
    SameProcessGroup,
}

#[async_trait::async_trait]
pub trait Execute {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error>;
}

#[async_trait::async_trait]
trait ExecuteInPipeline {
    async fn execute_in_pipeline(
        &self,
        context: PipelineExecutionContext<'_>,
        params: ExecutionParameters,
    ) -> Result<ExecutionSpawnResult, error::Error>;
}

#[async_trait::async_trait]
impl Execute for ast::Program {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        for command in &self.complete_commands {
            // Execute the command and handle any errors without immediately propagating them.
            // This allows interactive shells to continue executing subsequent commands even after
            // errors.
            match command.execute(shell, params).await {
                Ok(exec_result) => result = exec_result,
                Err(err) => {
                    // Display the error and convert to an execution result.
                    let _ = shell.display_error(&mut params.stderr(shell), &err).await;
                    result = err.into_result(shell);
                }
            }

            shell.set_last_exit_status(result.exit_code.into());

            // Check if we should stop executing subsequent commands
            if !result.is_normal_flow() {
                break;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::CompoundList {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        for ast::CompoundListItem(ao_list, sep) in &self.0 {
            let run_async = matches!(sep, ast::SeparatorOperator::Async);

            if run_async {
                // TODO(execute): Reenable launching in child process?
                // let job = spawn_ao_list_in_child(ao_list, shell, params).await?;

                let job = spawn_ao_list_in_task(ao_list, shell, params);
                let job_formatted = job.to_pid_style_string();

                if shell.options.interactive && !shell.is_subshell() {
                    writeln!(params.stderr(shell), "{job_formatted}")?;
                }

                result = ExecutionResult::success();
            } else {
                result = ao_list.execute(shell, params).await?;
            }

            if !result.is_normal_flow() {
                break;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

fn spawn_ao_list_in_task<'a>(
    ao_list: &ast::AndOrList,
    shell: &'a mut Shell,
    params: &ExecutionParameters,
) -> &'a jobs::Job {
    // Clone the inputs.
    let mut cloned_shell = shell.clone();
    let cloned_params = params.clone();
    let cloned_ao_list = ao_list.clone();

    // Mark the child shell as not interactive; we don't want it messing with the terminal too much.
    cloned_shell.options.interactive = false;

    let join_handle = tokio::spawn(async move {
        cloned_ao_list
            .execute(&mut cloned_shell, &cloned_params)
            .await
    });

    shell.jobs.add_as_current(jobs::Job::new(
        [jobs::JobTask::Internal(join_handle)],
        ao_list.to_string(),
        jobs::JobState::Running,
    ))
}

#[async_trait::async_trait]
impl Execute for ast::AndOrList {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = self.first.execute(shell, params).await?;

        for next_ao in &self.additional {
            // Check for non-normal control flow.
            if !result.is_normal_flow() {
                break;
            }

            let (is_and, pipeline) = match next_ao {
                ast::AndOr::And(p) => (true, p),
                ast::AndOr::Or(p) => (false, p),
            };

            // If we short-circuit, then we don't break out of the whole loop
            // but we skip evaluating the current pipeline. We'll then continue
            // on and possibly evaluate a subsequent one (depending on the
            // operator before it).
            if is_and {
                if !result.is_success() {
                    continue;
                }
            } else if result.is_success() {
                continue;
            }

            result = pipeline.execute(shell, params).await?;
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::Pipeline {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // Capture current timing if so requested.
        let stopwatch = self
            .timed
            .is_some()
            .then(timing::start_timing)
            .transpose()?;

        // Spawn all the processes required for the pipeline, connecting outputs/inputs with pipes
        // as needed.
        let spawn_results = spawn_pipeline_processes(self, shell, params).await?;

        // Wait for the processes. This also has a side effect of updating pipeline status.
        let mut result =
            wait_for_pipeline_processes_and_update_status(self, spawn_results, shell, params)
                .await?;

        // Invert the exit code if requested.
        if self.bang {
            result.exit_code = ExecutionExitCode::from(if result.is_success() { 1 } else { 0 });
        }

        // Update statuses.
        shell.set_last_exit_status(result.exit_code.into());

        // If requested, report timing.
        if let Some(timed) = &self.timed {
            if let Some(mut stderr) = params.try_fd(shell, openfiles::OpenFiles::STDERR_FD) {
                let timing = stopwatch.unwrap().stop()?;

                if timed.is_posix_output() {
                    std::write!(
                        stderr,
                        "real {}\nuser {}\nsys {}\n",
                        timing::format_duration_posixly(&timing.wall),
                        timing::format_duration_posixly(&timing.user),
                        timing::format_duration_posixly(&timing.system),
                    )?;
                } else {
                    std::write!(
                        stderr,
                        "\nreal\t{}\nuser\t{}\nsys\t{}\n",
                        timing::format_duration_non_posixly(&timing.wall),
                        timing::format_duration_non_posixly(&timing.user),
                        timing::format_duration_non_posixly(&timing.system),
                    )?;
                }
            }
        }

        Ok(result)
    }
}

async fn spawn_pipeline_processes(
    pipeline: &ast::Pipeline,
    shell: &mut Shell,
    params: &ExecutionParameters,
) -> Result<VecDeque<ExecutionSpawnResult>, error::Error> {
    let pipeline_len = pipeline.seq.len();
    let mut pipe_readers = vec![];
    let mut pipe_writers = vec![];
    let mut spawn_results = VecDeque::new();
    let mut process_group_id: Option<i32> = None;

    // Create pipes to use between commands, but only bother doing so if there's more than one command.
    if pipeline_len > 1 {
        for _ in 0..(pipeline_len - 1) {
            let (reader, writer) = std::io::pipe()?;
            pipe_readers.push(Some(reader.into()));
            pipe_writers.push(Some(writer.into()));
        }
        // Push `None` to the readers; it will be popped off by the *first* command, which will
        // mean that command gets its stdin from the execution parameters' current stdin.
        pipe_readers.push(None);
    }

    for (current_pipeline_index, command) in pipeline.seq.iter().enumerate() {
        //
        // We run a command directly in the current shell if either of the following is true:
        //     * There's only one command in the pipeline.
        //     * This is the *last* command in the pipeline, the lastpipe option is enabled, and job
        //       monitoring is disabled.
        // Otherwise, we spawn a separate subshell for each command in the pipeline.
        //

        let run_in_current_shell = pipeline_len == 1
            || (current_pipeline_index == pipeline_len - 1
                && shell.options.run_last_pipeline_cmd_in_current_shell
                && !shell.options.enable_job_control);

        // Set up parameters appropriate for this command.
        let mut cmd_params = params.clone();

        // Install pipes.
        if let Some(Some(reader)) = pipe_readers.pop() {
            cmd_params.open_files.set_fd(OpenFiles::STDIN_FD, reader);
        }
        if let Some(Some(writer)) = pipe_writers.pop() {
            cmd_params.open_files.set_fd(OpenFiles::STDOUT_FD, writer);
        }

        let pipeline_context = if !run_in_current_shell {
            // Make sure that all commands in the pipeline are in the same process group.
            if current_pipeline_index > 0 {
                cmd_params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;
            }

            PipelineExecutionContext {
                shell: commands::ShellForCommand::OwnedShell {
                    target: Box::new(shell.clone()),
                    parent: shell,
                },
                process_group_id,
            }
        } else {
            PipelineExecutionContext {
                shell: commands::ShellForCommand::ParentShell(shell),
                process_group_id,
            }
        };

        let spawn_result = command
            .execute_in_pipeline(pipeline_context, cmd_params)
            .await?;

        // Update the process group ID if something was spawned.
        if let ExecutionSpawnResult::StartedProcess(child) = &spawn_result {
            if process_group_id.is_none() {
                process_group_id = child.pgid();
            }
        }

        spawn_results.push_back(spawn_result);
    }

    Ok(spawn_results)
}

async fn wait_for_pipeline_processes_and_update_status(
    pipeline: &ast::Pipeline,
    mut process_spawn_results: VecDeque<ExecutionSpawnResult>,
    shell: &mut Shell,
    params: &ExecutionParameters,
) -> Result<ExecutionResult, error::Error> {
    let mut result = ExecutionResult::success();
    let mut stopped_children = vec![];

    // Clear our the pipeline status so we can start filling it out.
    shell.last_pipeline_statuses.clear();

    while let Some(child) = process_spawn_results.pop_front() {
        let wait_result = if !stopped_children.is_empty() {
            child.poll().await?
        } else {
            child.wait().await?
        };

        match wait_result {
            ExecutionWaitResult::Completed(current_result) => {
                result = current_result;
                shell.set_last_exit_status(result.exit_code.into());
                shell.last_pipeline_statuses.push(result.exit_code.into());
            }
            ExecutionWaitResult::Stopped(child) => {
                result = ExecutionResult::stopped();
                shell.set_last_exit_status(result.exit_code.into());
                shell.last_pipeline_statuses.push(result.exit_code.into());

                stopped_children.push(jobs::JobTask::External(child));
            }
        }
    }

    if shell.options.interactive {
        sys::terminal::move_self_to_foreground()?;
    }

    // If there were stopped jobs, then encapsulate the pipeline as a managed job and hand it
    // off to the job manager.
    if !stopped_children.is_empty() {
        let job = shell.jobs.add_as_current(jobs::Job::new(
            stopped_children,
            pipeline.to_string(),
            jobs::JobState::Stopped,
        ));

        let formatted = job.to_string();

        // N.B. We use the '\r' to overwrite any ^Z output.
        writeln!(params.stderr(shell), "\r{formatted}")?;
    }

    Ok(result)
}

#[async_trait::async_trait]
impl ExecuteInPipeline for ast::Command {
    async fn execute_in_pipeline(
        &self,
        mut pipeline_context: PipelineExecutionContext<'_>,
        mut params: ExecutionParameters,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        if pipeline_context.shell.options.do_not_execute_commands {
            return Ok(ExecutionSpawnResult::Completed(ExecutionResult::success()));
        }

        // Updates the shell with information about the currently executing command.
        pipeline_context.shell.set_current_cmd(self);

        match self {
            Self::Simple(simple) => simple.execute_in_pipeline(pipeline_context, params).await,
            Self::Compound(compound, redirects) => {
                // Set up any additional redirects.
                if let Some(redirects) = redirects {
                    for redirect in &redirects.0 {
                        setup_redirect(&mut pipeline_context.shell, &mut params, redirect).await?;
                    }
                }

                Ok(compound
                    .execute(&mut pipeline_context.shell, &params)
                    .await?
                    .into())
            }
            Self::Function(func) => Ok(func
                .execute(&mut pipeline_context.shell, &params)
                .await?
                .into()),
            Self::ExtendedTest(e, redirects) => {
                // Set up any additional redirects.
                if let Some(redirects) = redirects {
                    for redirect in &redirects.0 {
                        setup_redirect(&mut pipeline_context.shell, &mut params, redirect).await?;
                    }
                }

                // Evaluate the extended test expression.
                let result = if extendedtests::eval_extended_test_expr(
                    &e.expr,
                    &mut pipeline_context.shell,
                    &params,
                )
                .await?
                {
                    0
                } else {
                    1
                };
                Ok(ExecutionResult::new(result).into())
            }
        }
    }
}

enum WhileOrUntil {
    While,
    Until,
}

#[async_trait::async_trait]
impl Execute for ast::CompoundCommand {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        match self {
            Self::BraceGroup(ast::BraceGroupCommand { list, .. }) => {
                list.execute(shell, params).await
            }
            Self::Subshell(ast::SubshellCommand { list, .. }) => {
                // Clone off a new subshell, and run the body of the subshell there.
                // TODO(source-info): Do we need to reset the line number?
                let mut subshell = shell.clone();

                // Handle errors within the subshell context to prevent fatal errors
                // from propagating to the parent shell.
                let subshell_result = match list.execute(&mut subshell, params).await {
                    Ok(result) => result,
                    Err(error) => {
                        // Display the error to stderr, but prevent fatal error propagation
                        let mut stderr = params.stderr(shell);
                        let _ = shell.display_error(&mut stderr, &error).await;

                        // Convert error to result in subshell context
                        error.into_result(&subshell)
                    }
                };

                // Preserve the subshell's exit code, but don't honor any of its requests to exit
                // the shell, break out of loops, etc.
                Ok(ExecutionResult::from(subshell_result.exit_code))
            }
            Self::ForClause(f) => f.execute(shell, params).await,
            Self::CaseClause(c) => c.execute(shell, params).await,
            Self::IfClause(i) => i.execute(shell, params).await,
            Self::WhileClause(w) => (WhileOrUntil::While, w).execute(shell, params).await,
            Self::UntilClause(u) => (WhileOrUntil::Until, u).execute(shell, params).await,
            Self::Arithmetic(a) => a.execute(shell, params).await,
            Self::ArithmeticForClause(a) => a.execute(shell, params).await,
        }
    }
}

#[async_trait::async_trait]
impl Execute for ast::ForClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();

        // If we were given explicit words to iterate over, then expand them all, with splitting
        // enabled.
        let mut expanded_values = vec![];
        if let Some(unexpanded_values) = &self.values {
            for value in unexpanded_values {
                let mut expanded =
                    expansion::full_expand_and_split_word(shell, params, value).await?;
                expanded_values.append(&mut expanded);
            }
        } else {
            // Otherwise, we use the current positional parameters.
            expanded_values.extend_from_slice(shell.current_shell_args());
        }

        for value in expanded_values {
            if shell.options.print_commands_and_arguments {
                if let Some(unexpanded_values) = &self.values {
                    shell
                        .trace_command(
                            params,
                            std::format!(
                                "for {} in {}",
                                self.variable_name,
                                unexpanded_values.iter().join(" ")
                            ),
                        )
                        .await?;
                } else {
                    shell
                        .trace_command(params, std::format!("for {}", self.variable_name,))
                        .await?;
                }
            }

            // Update the variable.
            shell.env.update_or_add(
                &self.variable_name,
                ShellValueLiteral::Scalar(value),
                |_| Ok(()),
                EnvironmentLookup::Anywhere,
                EnvironmentScope::Global,
            )?;

            result = self.body.list.execute(shell, params).await?;
            if result.is_return_or_exit() {
                break;
            }

            let is_break = result.is_break();

            result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();

            if is_break || result.is_continue() {
                break;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::CaseClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // N.B. One would think it makes sense to trace the expanded value being switched
        // on, but that's not it.
        if shell.options.print_commands_and_arguments {
            shell
                .trace_command(params, std::format!("case {} in", &self.value))
                .await?;
        }

        let expanded_value = expansion::basic_expand_word(shell, params, &self.value).await?;
        let mut result: ExecutionResult = ExecutionResult::success();
        let mut force_execute_next_case = false;

        for case in &self.cases {
            if force_execute_next_case {
                force_execute_next_case = false;
            } else {
                let mut matches = false;
                for pattern in &case.patterns {
                    let expanded_pattern = expansion::basic_expand_pattern(shell, params, pattern)
                        .await?
                        .set_extended_globbing(shell.options.extended_globbing)
                        .set_case_insensitive(shell.options.case_insensitive_conditionals);

                    if expanded_pattern.exactly_matches(expanded_value.as_str())? {
                        matches = true;
                        break;
                    }
                }

                if !matches {
                    continue;
                }
            }

            result = if let Some(case_cmd) = &case.cmd {
                case_cmd.execute(shell, params).await?
            } else {
                ExecutionResult::success()
            };

            // Check for early return (return/exit) or loop control flow (break/continue)
            if !result.is_normal_flow() {
                break;
            }

            match case.post_action {
                ast::CaseItemPostAction::ExitCase => break,
                ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem => {
                    force_execute_next_case = true;
                }
                ast::CaseItemPostAction::ContinueEvaluatingCases => (),
            }
        }

        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::IfClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let condition = self.condition.execute(shell, params).await?;

        // Check if the condition itself resulted in non-normal control flow.
        if !condition.is_normal_flow() {
            return Ok(condition);
        }

        if condition.is_success() {
            return self.then.execute(shell, params).await;
        }

        if let Some(elses) = &self.elses {
            for else_clause in elses {
                match &else_clause.condition {
                    Some(else_condition) => {
                        let else_condition_result = else_condition.execute(shell, params).await?;

                        // Check if the elif condition caused non-normal control flow.
                        if !else_condition_result.is_normal_flow() {
                            return Ok(else_condition_result);
                        }

                        if else_condition_result.is_success() {
                            return else_clause.body.execute(shell, params).await;
                        }
                    }
                    None => {
                        return else_clause.body.execute(shell, params).await;
                    }
                }
            }
        }

        let result = ExecutionResult::success();
        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for (WhileOrUntil, &ast::WhileOrUntilClauseCommand) {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let is_while = match self.0 {
            WhileOrUntil::While => true,
            WhileOrUntil::Until => false,
        };
        let test_condition = &self.1.0;
        let body = &self.1.1;

        let mut result = ExecutionResult::success();

        loop {
            let condition_result = test_condition.execute(shell, params).await?;
            if !condition_result.is_normal_flow() {
                result = condition_result;

                // If the condition has break/continue, the while/until loop itself
                // consumes one level. We need to decrement the level before returning.
                result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();
                break;
            }

            if condition_result.is_success() != is_while {
                break;
            }

            result = body.list.execute(shell, params).await?;
            if result.is_return_or_exit() {
                break;
            }

            let is_break = result.is_break();

            result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();

            if is_break || result.is_continue() {
                break;
            }
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::ArithmeticCommand {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let value = self.expr.eval(shell, params, true).await?;
        let result = if value != 0 {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        };

        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::ArithmeticForClauseCommand {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = ExecutionResult::success();
        if let Some(initializer) = &self.initializer {
            initializer.eval(shell, params, true).await?;
        }

        loop {
            if let Some(condition) = &self.condition {
                if condition.eval(shell, params, true).await? == 0 {
                    break;
                }
            }

            result = self.body.list.execute(shell, params).await?;
            if result.is_return_or_exit() {
                break;
            }

            let is_break = result.is_break();

            result.next_control_flow = result.next_control_flow.try_decrement_loop_levels();

            if is_break || result.is_continue() {
                break;
            }

            if let Some(updater) = &self.updater {
                updater.eval(shell, params, true).await?;
            }
        }

        shell.set_last_exit_status(result.exit_code.into());
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Execute for ast::FunctionDefinition {
    async fn execute(
        &self,
        shell: &mut Shell,
        _params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        // The function definition's source context should be the same as the current frame
        // so we directly pass that through.
        let source_info = shell
            .call_stack()
            .current_frame()
            .map_or_else(crate::SourceInfo::default, |frame| {
                frame.adjusted_source_info()
            });
        shell.define_func(self.fname.value.clone(), self.clone(), &source_info);

        let result = ExecutionResult::success();
        shell.set_last_exit_status(result.exit_code.into());

        Ok(result)
    }
}

#[async_trait::async_trait]
#[allow(clippy::too_many_lines)]
impl ExecuteInPipeline for ast::SimpleCommand {
    async fn execute_in_pipeline(
        &self,
        mut context: PipelineExecutionContext<'_>,
        mut params: ExecutionParameters,
    ) -> Result<ExecutionSpawnResult, error::Error> {
        let prefix_iter = self.prefix.as_ref().map(|s| s.0.iter()).unwrap_or_default();
        let suffix_iter = self.suffix.as_ref().map(|s| s.0.iter()).unwrap_or_default();
        let cmd_name_items = self
            .word_or_name
            .as_ref()
            .map(|won| CommandPrefixOrSuffixItem::Word(won.clone()));

        let mut assignments = vec![];
        let mut args: Vec<CommandArg> = vec![];
        let mut command_takes_assignments = false;

        for item in prefix_iter.chain(cmd_name_items.iter()).chain(suffix_iter) {
            match item {
                CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                    if let Err(e) = setup_redirect(&mut context.shell, &mut params, redirect).await
                    {
                        writeln!(params.stderr(&context.shell), "error: {e}")?;
                        return Ok(ExecutionResult::general_error().into());
                    }
                }
                CommandPrefixOrSuffixItem::ProcessSubstitution(kind, subshell_command) => {
                    let (installed_fd_num, substitution_file) = setup_process_substitution(
                        &context.shell,
                        &params,
                        kind,
                        subshell_command,
                    )?;

                    params
                        .open_files
                        .set_fd(installed_fd_num, substitution_file);

                    args.push(CommandArg::String(std::format!(
                        "/dev/fd/{installed_fd_num}"
                    )));
                }
                CommandPrefixOrSuffixItem::AssignmentWord(assignment, word) => {
                    if args.is_empty() {
                        // If we haven't yet seen any arguments, then this must be a proper
                        // scoped assignment. Add it to the list we're accumulating.
                        assignments.push(assignment);
                    } else {
                        if command_takes_assignments {
                            // This looks like an assignment, and the command being invoked is a
                            // well-known builtin that takes arguments that need to function like
                            // assignments (but which are processed by the builtin).
                            let expanded =
                                expand_assignment(&mut context.shell, &params, assignment).await?;
                            args.push(CommandArg::Assignment(expanded));
                        } else {
                            // This *looks* like an assignment, but it's really a string we should
                            // fully treat as a regular looking
                            // argument.
                            let mut next_args = expansion::full_expand_and_split_word(
                                &mut context.shell,
                                &params,
                                word,
                            )
                            .await?
                            .into_iter()
                            .map(CommandArg::String)
                            .collect();
                            args.append(&mut next_args);
                        }
                    }
                }
                CommandPrefixOrSuffixItem::Word(arg) => {
                    let mut next_args =
                        expansion::full_expand_and_split_word(&mut context.shell, &params, arg)
                            .await?;

                    if args.is_empty() {
                        if let Some(cmd_name) = next_args.first() {
                            if let Some(alias_value) = context.shell.aliases.get(cmd_name.as_str())
                            {
                                //
                                // TODO(#57): This is a total hack; aliases are supposed to be
                                // handled much earlier in the process.
                                //
                                let mut alias_pieces: Vec<_> = alias_value
                                    .split_ascii_whitespace()
                                    .map(|i| i.to_owned())
                                    .collect();

                                next_args.remove(0);
                                alias_pieces.append(&mut next_args);

                                next_args = alias_pieces;
                            }

                            let first_arg = next_args[0].as_str();

                            // Check if we're going to be invoking a special declaration builtin.
                            // That will change how we parse and process args.
                            if context
                                .shell
                                .builtins()
                                .get(first_arg)
                                .is_some_and(|r| !r.disabled && r.declaration_builtin)
                            {
                                command_takes_assignments = true;
                            }
                        }
                    }

                    let mut next_args = next_args.into_iter().map(CommandArg::String).collect();
                    args.append(&mut next_args);
                }
            }
        }

        // If we have a command, then execute it.
        if let Some(CommandArg::String(cmd_name)) = args.first().cloned() {
            let mut stderr = params.stderr(&context.shell);

            let (owned_shell, parent_shell) = match context.shell {
                commands::ShellForCommand::ParentShell(shell) => (None, shell),
                commands::ShellForCommand::OwnedShell { target, parent } => (Some(target), parent),
            };

            let shell = if let Some(owned_shell) = owned_shell {
                commands::ShellForCommand::OwnedShell {
                    target: owned_shell,
                    parent: parent_shell,
                }
            } else {
                commands::ShellForCommand::ParentShell(parent_shell)
            };

            let context = PipelineExecutionContext {
                shell,
                process_group_id: context.process_group_id,
            };

            match execute_command(context, params, cmd_name, assignments, args).await {
                Ok(result) => Ok(result),
                Err(err) => {
                    let _ = parent_shell.display_error(&mut stderr, &err).await;

                    let result = err.into_result(parent_shell);
                    Ok(result.into())
                }
            }
        } else {
            let status_change_count = context.shell.last_exit_status_change_count();

            // No command to run; assignments must be applied to this shell.
            for assignment in assignments {
                // Apply the assignment. Don't mark as fatal - let errors be handled
                // at the program level so multiple complete_commands can execute independently.
                apply_assignment(
                    assignment,
                    &mut context.shell,
                    &params,
                    false,
                    None,
                    EnvironmentScope::Global,
                )
                .await?;
            }

            // We need to set the last exit status to indicate assignment success,
            // but only if there was no status set during expansion. Ideally we'd
            // have a cleaner way of determining if expansion caused a status change,
            // but this will do for now.
            if status_change_count == context.shell.last_exit_status_change_count() {
                context.shell.set_last_exit_status(0);
            }

            // Return the last exit status we have; in some cases, an expansion
            // might result in a non-zero exit status stored in the shell.
            Ok(ExecutionResult::new(context.shell.last_exit_status()).into())
        }
    }
}

async fn execute_command(
    mut context: PipelineExecutionContext<'_>,
    params: ExecutionParameters,
    cmd_name: String,
    assignments: Vec<&ast::Assignment>,
    args: Vec<CommandArg>,
) -> Result<ExecutionSpawnResult, error::Error> {
    // Push a new ephemeral environment scope for the duration of the command. We'll
    // set command-scoped variable assignments after doing so, and revert them before
    // returning.
    context.shell.env.push_scope(EnvironmentScope::Command);

    for assignment in &assignments {
        // Ensure it's tagged as exported and created in the command scope.
        apply_assignment(
            assignment,
            &mut context.shell,
            &params,
            true,
            Some(EnvironmentScope::Command),
            EnvironmentScope::Command,
        )
        .await?;
    }

    if context.shell.options.print_commands_and_arguments {
        context
            .shell
            .trace_command(
                &params,
                args.iter().map(|arg| arg.quote_for_tracing()).join(" "),
            )
            .await?;
    }

    // Construct the command struct.
    let mut cmd = commands::SimpleCommand::new(context.shell, params, cmd_name, args);
    cmd.process_group_id = context.process_group_id;

    // Arrange to pop off that ephemeral environment scope.
    cmd.post_execute = Some(|shell| shell.env.pop_scope(EnvironmentScope::Command));

    // Run through any pre-execution hooks.
    commands::on_preexecute(&mut cmd).await?;

    // Execute
    // TODO(jobs): do we need to move self back to foreground on error here?
    cmd.execute().await
}

async fn expand_assignment(
    shell: &mut Shell,
    params: &ExecutionParameters,
    assignment: &ast::Assignment,
) -> Result<ast::Assignment, error::Error> {
    let value = expand_assignment_value(shell, params, &assignment.value).await?;
    Ok(ast::Assignment {
        name: basic_expand_assignment_name(shell, params, &assignment.name).await?,
        value,
        append: assignment.append,
        loc: assignment.loc.clone(),
    })
}

async fn basic_expand_assignment_name(
    shell: &mut Shell,
    params: &ExecutionParameters,
    name: &ast::AssignmentName,
) -> Result<ast::AssignmentName, error::Error> {
    match name {
        ast::AssignmentName::VariableName(name) => {
            let expanded = expansion::basic_expand_str(shell, params, name).await?;
            Ok(ast::AssignmentName::VariableName(expanded))
        }
        ast::AssignmentName::ArrayElementName(name, index) => {
            let expanded_name = expansion::basic_expand_str(shell, params, name).await?;
            let expanded_index = expansion::basic_expand_str(shell, params, index).await?;
            Ok(ast::AssignmentName::ArrayElementName(
                expanded_name,
                expanded_index,
            ))
        }
    }
}

async fn expand_assignment_value(
    shell: &mut Shell,
    params: &ExecutionParameters,
    value: &ast::AssignmentValue,
) -> Result<ast::AssignmentValue, error::Error> {
    let expanded = match value {
        ast::AssignmentValue::Scalar(s) => {
            let expanded_word = expansion::basic_expand_word(shell, params, s).await?;
            ast::AssignmentValue::Scalar(ast::Word::from(expanded_word))
        }
        ast::AssignmentValue::Array(arr) => {
            let mut expanded_values = vec![];
            for (key, value) in arr {
                if let Some(k) = key {
                    let expanded_key = expansion::basic_expand_word(shell, params, k).await?.into();
                    let expanded_value = expansion::basic_expand_word(shell, params, value)
                        .await?
                        .into();
                    expanded_values.push((Some(expanded_key), expanded_value));
                } else {
                    let split_expanded_value =
                        expansion::full_expand_and_split_word(shell, params, value).await?;
                    for expanded_value in split_expanded_value {
                        expanded_values.push((None, expanded_value.into()));
                    }
                }
            }

            ast::AssignmentValue::Array(expanded_values)
        }
    };

    Ok(expanded)
}

#[expect(clippy::too_many_lines)]
async fn apply_assignment(
    assignment: &ast::Assignment,
    shell: &mut Shell,
    params: &ExecutionParameters,
    mut export: bool,
    required_scope: Option<EnvironmentScope>,
    creation_scope: EnvironmentScope,
) -> Result<(), error::Error> {
    // Figure out if we are trying to assign to a variable or assign to an element of an existing
    // array.
    let mut array_index;
    let variable_name = match &assignment.name {
        ast::AssignmentName::VariableName(name) => {
            array_index = None;
            name
        }
        ast::AssignmentName::ArrayElementName(name, index) => {
            let expanded = expansion::basic_expand_str(shell, params, index).await?;
            array_index = Some(expanded);
            name
        }
    };

    // Expand the values.
    let new_value = match &assignment.value {
        ast::AssignmentValue::Scalar(unexpanded_value) => {
            let value = expansion::basic_expand_word(shell, params, unexpanded_value).await?;
            ShellValueLiteral::Scalar(value)
        }
        ast::AssignmentValue::Array(unexpanded_values) => {
            let mut elements = vec![];
            for (unexpanded_key, unexpanded_value) in unexpanded_values {
                let key = match unexpanded_key {
                    Some(unexpanded_key) => {
                        Some(expansion::basic_expand_word(shell, params, unexpanded_key).await?)
                    }
                    None => None,
                };

                if key.is_some() {
                    let value =
                        expansion::basic_expand_word(shell, params, unexpanded_value).await?;
                    elements.push((key, value));
                } else {
                    let values =
                        expansion::full_expand_and_split_word(shell, params, unexpanded_value)
                            .await?;
                    for value in values {
                        elements.push((None, value));
                    }
                }
            }
            ShellValueLiteral::Array(ArrayLiteral(elements))
        }
    };

    if shell.options.print_commands_and_arguments {
        let op = if assignment.append { "+=" } else { "=" };
        shell
            .trace_command(params, std::format!("{}{op}{new_value}", assignment.name))
            .await?;
    }

    // See if we need to eval an array index.
    if let Some(idx) = &array_index {
        let will_be_indexed_array = if let Some((_, existing_value)) = shell.env.get(variable_name)
        {
            matches!(
                existing_value.value(),
                ShellValue::IndexedArray(_) | ShellValue::Unset(ShellValueUnsetType::IndexedArray)
            )
        } else {
            true
        };

        if will_be_indexed_array {
            array_index = Some(
                arithmetic::expand_and_eval(shell, params, idx.as_str(), false)
                    .await?
                    .to_string(),
            );
        }
    }

    // See if we can find an existing value associated with the variable.
    if let Some((existing_value_scope, existing_value)) = shell.env.get_mut(variable_name.as_str())
    {
        if required_scope.is_none() || Some(existing_value_scope) == required_scope {
            if let Some(array_index) = array_index {
                match new_value {
                    ShellValueLiteral::Scalar(s) => {
                        existing_value.assign_at_index(array_index, s, assignment.append)?;
                    }
                    ShellValueLiteral::Array(_) => {
                        return error::unimp("replacing an array item with an array");
                    }
                }
            } else {
                if !export
                    && shell.options.export_variables_on_modification
                    && !matches!(new_value, ShellValueLiteral::Array(_))
                {
                    export = true;
                }

                existing_value.assign(new_value, assignment.append)?;
            }

            if export {
                existing_value.export();
            }

            // That's it!
            return Ok(());
        }
    }

    // If we fell down here, then we need to add it.
    let new_value = if let Some(array_index) = array_index {
        match new_value {
            ShellValueLiteral::Scalar(s) => {
                ShellValue::indexed_array_from_literals(ArrayLiteral(vec![(Some(array_index), s)]))
            }
            ShellValueLiteral::Array(_) => {
                return error::unimp("cannot assign list to array member");
            }
        }
    } else {
        match new_value {
            ShellValueLiteral::Scalar(s) => {
                export = export || shell.options.export_variables_on_modification;
                ShellValue::String(s)
            }
            ShellValueLiteral::Array(values) => ShellValue::indexed_array_from_literals(values),
        }
    };

    let mut new_var = ShellVariable::new(new_value);

    if export {
        new_var.export();
    }

    shell.env.add(variable_name, new_var, creation_scope)
}

#[expect(clippy::too_many_lines)]
pub(crate) async fn setup_redirect(
    shell: &mut Shell,
    params: &'_ mut ExecutionParameters,
    redirect: &ast::IoRedirect,
) -> Result<(), error::Error> {
    match redirect {
        ast::IoRedirect::OutputAndError(f, append) => {
            let mut expanded_fields =
                expansion::full_expand_and_split_word(shell, params, f).await?;
            if expanded_fields.len() != 1 {
                return Err(error::ErrorKind::InvalidRedirection.into());
            }

            let expanded_file_path: PathBuf =
                shell.absolute_path(Path::new(expanded_fields.remove(0).as_str()));

            let mut file_options = std::fs::File::options();
            file_options
                .create(true)
                .write(true)
                .truncate(!*append)
                .append(*append);

            let stdout_file = shell
                .open_file(&file_options, &expanded_file_path, params)
                .map_err(|err| {
                    error::ErrorKind::RedirectionFailure(
                        expanded_file_path.to_string_lossy().to_string(),
                        err.to_string(),
                    )
                })?;

            let stderr_file = stdout_file.try_clone()?;

            params.open_files.set_fd(OpenFiles::STDOUT_FD, stdout_file);
            params.open_files.set_fd(OpenFiles::STDERR_FD, stderr_file);
        }

        ast::IoRedirect::File(specified_fd_num, kind, target) => {
            match target {
                ast::IoFileRedirectTarget::Filename(f) => {
                    let mut options = std::fs::File::options();

                    let mut expanded_fields =
                        expansion::full_expand_and_split_word(shell, params, f).await?;

                    if expanded_fields.len() != 1 {
                        return Err(error::ErrorKind::InvalidRedirection.into());
                    }

                    let expanded_file_path: PathBuf =
                        shell.absolute_path(Path::new(expanded_fields.remove(0).as_str()));

                    let default_fd_if_unspecified = get_default_fd_for_redirect_kind(kind);
                    match kind {
                        ast::IoFileRedirectKind::Read => {
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::Write => {
                            if shell
                                .options
                                .disallow_overwriting_regular_files_via_output_redirection
                            {
                                // First check to see if the path points to an existing regular
                                // file.
                                if !expanded_file_path.is_file() {
                                    options.create(true);
                                } else {
                                    options.create_new(true);
                                }
                                options.write(true);
                            } else {
                                options.create(true);
                                options.write(true);
                                options.truncate(true);
                            }
                        }
                        ast::IoFileRedirectKind::Append => {
                            options.create(true);
                            options.append(true);
                        }
                        ast::IoFileRedirectKind::ReadAndWrite => {
                            options.create(true);
                            options.read(true);
                            options.write(true);
                        }
                        ast::IoFileRedirectKind::Clobber => {
                            options.create(true);
                            options.write(true);
                            options.truncate(true);
                        }
                        ast::IoFileRedirectKind::DuplicateInput => {
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::DuplicateOutput => {
                            options.create(true);
                            options.write(true);
                        }
                    }

                    let fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    let opened_file = shell
                        .open_file(&options, &expanded_file_path, params)
                        .map_err(|err| {
                            error::ErrorKind::RedirectionFailure(
                                expanded_file_path.to_string_lossy().to_string(),
                                err.to_string(),
                            )
                        })?;

                    params.open_files.set_fd(fd_num, opened_file);
                }

                ast::IoFileRedirectTarget::Fd(fd) => {
                    let default_fd_if_unspecified = match kind {
                        ast::IoFileRedirectKind::DuplicateInput => 0,
                        ast::IoFileRedirectKind::DuplicateOutput => 1,
                        _ => {
                            return error::unimp("unexpected redirect kind");
                        }
                    };

                    let fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    if let Some(f) = params.try_fd(shell, *fd) {
                        let target_file = f.try_clone()?;

                        params.open_files.set_fd(fd_num, target_file);
                    } else {
                        return Err(error::ErrorKind::BadFileDescriptor(*fd).into());
                    }
                }

                ast::IoFileRedirectTarget::Duplicate(word) => {
                    let default_fd_if_unspecified = match kind {
                        ast::IoFileRedirectKind::DuplicateInput => 0,
                        ast::IoFileRedirectKind::DuplicateOutput => 1,
                        _ => {
                            return error::unimp("unexpected redirect kind");
                        }
                    };

                    let fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    let mut expanded_fields =
                        expansion::full_expand_and_split_word(shell, params, word).await?;

                    if expanded_fields.len() != 1 {
                        return Err(error::ErrorKind::InvalidRedirection.into());
                    }

                    let mut expanded = expanded_fields.remove(0);

                    let dash = if expanded.ends_with('-') {
                        expanded.pop();
                        true
                    } else {
                        false
                    };

                    if expanded.is_empty() {
                        // Nothing to do
                    } else if expanded.chars().all(|c: char| c.is_ascii_digit()) {
                        let source_fd_num = expanded
                            .parse::<ShellFd>()
                            .map_err(|_| error::ErrorKind::InvalidRedirection)?;

                        // Duplicate the fd.
                        let target_file = if let Some(f) = params.try_fd(shell, source_fd_num) {
                            f.try_clone()?
                        } else {
                            return Err(error::ErrorKind::BadFileDescriptor(source_fd_num).into());
                        };

                        params.open_files.set_fd(fd_num, target_file);
                    } else {
                        return Err(error::ErrorKind::InvalidRedirection.into());
                    }

                    if dash {
                        // Close the specified fd. Ignore it if it's not valid.
                        params.open_files.remove_fd(fd_num);
                    }
                }

                ast::IoFileRedirectTarget::ProcessSubstitution(substitution_kind, subshell_cmd) => {
                    match kind {
                        ast::IoFileRedirectKind::Read
                        | ast::IoFileRedirectKind::Write
                        | ast::IoFileRedirectKind::Append
                        | ast::IoFileRedirectKind::ReadAndWrite
                        | ast::IoFileRedirectKind::Clobber => {
                            let (substitution_fd, substitution_file) = setup_process_substitution(
                                shell,
                                params,
                                substitution_kind,
                                subshell_cmd,
                            )?;

                            let target_file = substitution_file.try_clone()?;
                            params.open_files.set_fd(substitution_fd, substitution_file);

                            let fd_num = specified_fd_num
                                .unwrap_or_else(|| get_default_fd_for_redirect_kind(kind));

                            params.open_files.set_fd(fd_num, target_file);
                        }
                        _ => return error::unimp("invalid process substitution"),
                    }
                }
            }
        }

        ast::IoRedirect::HereDocument(fd_num, io_here) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            // Expand if required.
            let io_here_doc = if io_here.requires_expansion {
                expansion::basic_expand_word(shell, params, &io_here.doc).await?
            } else {
                io_here.doc.flatten()
            };

            let f = setup_open_file_with_contents(io_here_doc.as_str())?;

            params.open_files.set_fd(fd_num, f);
        }

        ast::IoRedirect::HereString(fd_num, word) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            let mut expanded_word = expansion::basic_expand_word(shell, params, word).await?;
            expanded_word.push('\n');

            let f = setup_open_file_with_contents(expanded_word.as_str())?;

            params.open_files.set_fd(fd_num, f);
        }
    }

    Ok(())
}

const fn get_default_fd_for_redirect_kind(kind: &ast::IoFileRedirectKind) -> ShellFd {
    match kind {
        ast::IoFileRedirectKind::Read => 0,
        ast::IoFileRedirectKind::Write => 1,
        ast::IoFileRedirectKind::Append => 1,
        ast::IoFileRedirectKind::ReadAndWrite => 0,
        ast::IoFileRedirectKind::Clobber => 1,
        ast::IoFileRedirectKind::DuplicateInput => 0,
        ast::IoFileRedirectKind::DuplicateOutput => 1,
    }
}

fn setup_process_substitution(
    shell: &Shell,
    params: &ExecutionParameters,
    kind: &ast::ProcessSubstitutionKind,
    subshell_cmd: &ast::SubshellCommand,
) -> Result<(ShellFd, OpenFile), error::Error> {
    // TODO(execute): Don't execute synchronously!
    // Execute in a subshell.
    let mut subshell = shell.clone();

    // Set up execution parameters for the child execution.
    let mut child_params = params.clone();
    child_params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

    // Set up pipe so we can connect to the command.
    let (reader, writer) = std::io::pipe()?;
    let (reader, writer) = (reader.into(), writer.into());

    let target_file = match kind {
        ast::ProcessSubstitutionKind::Read => {
            child_params.open_files.set_fd(OpenFiles::STDOUT_FD, writer);
            reader
        }
        ast::ProcessSubstitutionKind::Write => {
            child_params.open_files.set_fd(OpenFiles::STDIN_FD, reader);
            writer
        }
    };

    // Asynchronously spawn off the subshell; we intentionally don't block on its
    // completion.
    let subshell_cmd = subshell_cmd.to_owned();
    tokio::spawn(async move {
        // Intentionally ignore the result of the subshell command.
        let _ = subshell_cmd
            .list
            .execute(&mut subshell, &child_params)
            .await;
    });

    // Starting at 63 (a.k.a. 64-1)--and decrementing--look for an
    // available fd.
    let mut candidate_fd_num = 63;
    while params.open_files.contains_fd(candidate_fd_num) {
        candidate_fd_num -= 1;
        if candidate_fd_num == 0 {
            return error::unimp("no available file descriptors");
        }
    }

    Ok((candidate_fd_num, target_file))
}

fn setup_open_file_with_contents(contents: &str) -> Result<OpenFile, error::Error> {
    let (reader, mut writer) = std::io::pipe()?;

    let bytes = contents.as_bytes();

    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsFd as _;

        let len = i32::try_from(bytes.len())?;
        nix::fcntl::fcntl(reader.as_fd(), nix::fcntl::FcntlArg::F_SETPIPE_SZ(len))?;
    }

    writer.write_all(bytes)?;
    drop(writer);

    Ok(reader.into())
}
