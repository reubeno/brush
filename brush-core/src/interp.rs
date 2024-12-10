use brush_parser::ast::{self, CommandPrefixOrSuffixItem};
use itertools::Itertools;
use std::collections::VecDeque;
use std::io::Write;
#[cfg(target_os = "linux")]
use std::os::fd::{AsFd, AsRawFd};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::arithmetic::ExpandAndEvaluate;
use crate::commands::{self, CommandArg, CommandSpawnResult};
use crate::env::{EnvironmentLookup, EnvironmentScope};
use crate::openfiles::{OpenFile, OpenFiles};
use crate::shell::Shell;
use crate::variables::{
    ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
};
use crate::{error, expansion, extendedtests, jobs, openfiles, processes, sys, traps};

/// Encapsulates the result of executing a command.
#[derive(Debug, Default)]
pub struct ExecutionResult {
    /// The numerical exit code of the command.
    pub exit_code: u8,
    /// Whether the shell should exit after this command.
    pub exit_shell: bool,
    /// Whether the shell should return from the current function or script.
    pub return_from_function_or_script: bool,
    /// If the command was executed in a loop, this is the number of levels to break out of.
    pub break_loop: Option<u8>,
    /// If the command was executed in a loop, this is the number of levels to continue.
    pub continue_loop: Option<u8>,
}

impl From<processes::ProcessWaitResult> for ExecutionResult {
    fn from(wait_result: processes::ProcessWaitResult) -> Self {
        match wait_result {
            processes::ProcessWaitResult::Completed(output) => output.into(),
            processes::ProcessWaitResult::Stopped => Self::stopped(),
        }
    }
}

impl From<std::process::Output> for ExecutionResult {
    fn from(output: std::process::Output) -> ExecutionResult {
        if let Some(code) = output.status.code() {
            #[allow(clippy::cast_sign_loss)]
            return Self::new((code & 0xFF) as u8);
        }

        #[cfg(unix)]
        if let Some(signal) = output.status.signal() {
            #[allow(clippy::cast_sign_loss)]
            return Self::new((signal & 0xFF) as u8 + 128);
        }

        tracing::error!("unhandled process exit");
        Self::new(127)
    }
}

impl ExecutionResult {
    /// Returns a new `ExecutionResult` with the given exit code.
    ///
    /// # Parameters
    /// - `exit_code` - The exit code of the command.
    pub fn new(exit_code: u8) -> ExecutionResult {
        ExecutionResult {
            exit_code,
            ..ExecutionResult::default()
        }
    }

    /// Returns a new `ExecutionResult` with an exit code of 0.
    pub fn success() -> ExecutionResult {
        Self::new(0)
    }

    /// Returns whether the command was successful.
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Returns a new `ExecutionResult` reflecting a process that was stopped.
    pub fn stopped() -> ExecutionResult {
        // TODO: Decide how to sort this out in a platform-independent way.
        const SIGTSTP: std::os::raw::c_int = 20;

        #[allow(clippy::cast_possible_truncation)]
        Self::new(128 + SIGTSTP as u8)
    }
}

/// Encapsulates the context of execution in a command pipeline.
struct PipelineExecutionContext<'a> {
    /// The shell in which the command is being executed.
    shell: &'a mut Shell,

    current_pipeline_index: usize,
    pipeline_len: usize,
    output_pipes: &'a mut Vec<sys::pipes::PipeReader>,

    process_group_id: Option<i32>,

    params: ExecutionParameters,
}

/// Parameters for execution.
#[derive(Clone, Default)]
pub struct ExecutionParameters {
    /// The open files tracked by the current context.
    pub open_files: openfiles::OpenFiles,
    /// Policy for how to manage spawned external processes.
    pub process_group_policy: ProcessGroupPolicy,
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
        context: &mut PipelineExecutionContext,
    ) -> Result<CommandSpawnResult, error::Error>;
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
            result = command.execute(shell, params).await?;
            if result.exit_shell || result.return_from_function_or_script {
                break;
            }
        }

        shell.last_exit_status = result.exit_code;
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
                // TODO: Reenable launching in child process?
                // let job = spawn_ao_list_in_child(ao_list, shell, params).await?;

                let job = spawn_ao_list_in_task(ao_list, shell, params);
                let job_formatted = job.to_pid_style_string();

                if shell.options.interactive {
                    writeln!(shell.stderr(), "{job_formatted}")?;
                }

                result = ExecutionResult::success();
            } else {
                result = ao_list.execute(shell, params).await?;
            }

            // Check for early return.
            if result.return_from_function_or_script {
                break;
            }

            // TODO: Check for continue/break being in for/while/until loop.
            if result.continue_loop.is_some() || result.break_loop.is_some() {
                break;
            }
        }

        shell.last_exit_status = result.exit_code;
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

    let job = shell.jobs.add_as_current(jobs::Job::new(
        [jobs::JobTask::Internal(join_handle)],
        ao_list.to_string(),
        jobs::JobState::Running,
    ));

    job
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
            // Check for exit/return
            if result.exit_shell || result.return_from_function_or_script {
                break;
            }

            // Check for continue/break
            // TODO: Validate that we're in a loop?
            if result.continue_loop.is_some() || result.break_loop.is_some() {
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
        // Spawn all the processes required for the pipeline, connecting outputs/inputs with pipes
        // as needed.
        let spawn_results = spawn_pipeline_processes(self, shell, params).await?;

        // Wait for the processes.
        let mut result = wait_for_pipeline_processes(self, spawn_results, shell).await?;

        // Invert the exit code if requested.
        if self.bang {
            result.exit_code = if result.exit_code == 0 { 1 } else { 0 };
        }

        shell.last_exit_status = result.exit_code;

        Ok(result)
    }
}

async fn spawn_pipeline_processes(
    pipeline: &ast::Pipeline,
    shell: &mut Shell,
    params: &ExecutionParameters,
) -> Result<VecDeque<CommandSpawnResult>, error::Error> {
    let pipeline_len = pipeline.seq.len();
    let mut output_pipes = vec![];
    let mut spawn_results = VecDeque::new();
    let mut process_group_id: Option<i32> = None;

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

        if !run_in_current_shell {
            let mut subshell = shell.clone();
            let mut pipeline_context = PipelineExecutionContext {
                shell: &mut subshell,
                current_pipeline_index,
                pipeline_len,
                output_pipes: &mut output_pipes,
                process_group_id,
                params: params.clone(),
            };

            // Make sure that all commands in the pipeline are in the same process group.
            if current_pipeline_index > 0 {
                pipeline_context.params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;
            }

            spawn_results.push_back(command.execute_in_pipeline(&mut pipeline_context).await?);
            process_group_id = pipeline_context.process_group_id;
        } else {
            let mut pipeline_context = PipelineExecutionContext {
                shell,
                current_pipeline_index,
                pipeline_len,
                output_pipes: &mut output_pipes,
                process_group_id,
                params: params.clone(),
            };

            spawn_results.push_back(command.execute_in_pipeline(&mut pipeline_context).await?);
            process_group_id = pipeline_context.process_group_id;
        }
    }

    Ok(spawn_results)
}

async fn wait_for_pipeline_processes(
    pipeline: &ast::Pipeline,
    mut process_spawn_results: VecDeque<CommandSpawnResult>,
    shell: &mut Shell,
) -> Result<ExecutionResult, error::Error> {
    let mut result = ExecutionResult::success();
    let mut stopped_children = vec![];

    while let Some(child) = process_spawn_results.pop_front() {
        match child.wait(!stopped_children.is_empty()).await? {
            commands::CommandWaitResult::CommandCompleted(current_result) => {
                result = current_result;
                shell.last_exit_status = result.exit_code;
            }
            commands::CommandWaitResult::CommandStopped(current_result, child) => {
                result = current_result;
                shell.last_exit_status = result.exit_code;

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
        writeln!(shell.stderr(), "\r{formatted}")?;
    }

    Ok(result)
}

#[async_trait::async_trait]
impl ExecuteInPipeline for ast::Command {
    async fn execute_in_pipeline(
        &self,
        pipeline_context: &mut PipelineExecutionContext,
    ) -> Result<CommandSpawnResult, error::Error> {
        if pipeline_context.shell.options.do_not_execute_commands {
            return Ok(CommandSpawnResult::ImmediateExit(0));
        }

        match self {
            ast::Command::Simple(simple) => simple.execute_in_pipeline(pipeline_context).await,
            ast::Command::Compound(compound, redirects) => {
                let mut params = pipeline_context.params.clone();

                // Set up pipelining.
                setup_pipeline_redirection(&mut params.open_files, pipeline_context)?;

                // Set up any additional redirects.
                if let Some(redirects) = redirects {
                    for redirect in &redirects.0 {
                        setup_redirect(&mut params.open_files, pipeline_context.shell, redirect)
                            .await?;
                    }
                }

                let result = compound.execute(pipeline_context.shell, &params).await?;
                if result.exit_shell {
                    Ok(CommandSpawnResult::ExitShell(result.exit_code))
                } else if result.return_from_function_or_script {
                    Ok(CommandSpawnResult::ReturnFromFunctionOrScript(
                        result.exit_code,
                    ))
                } else if let Some(count) = result.break_loop {
                    Ok(CommandSpawnResult::BreakLoop(count))
                } else if let Some(count) = result.continue_loop {
                    Ok(CommandSpawnResult::ContinueLoop(count))
                } else {
                    Ok(CommandSpawnResult::ImmediateExit(result.exit_code))
                }
            }
            ast::Command::Function(func) => {
                let result = func
                    .execute(pipeline_context.shell, &pipeline_context.params)
                    .await?;
                Ok(CommandSpawnResult::ImmediateExit(result.exit_code))
            }
            ast::Command::ExtendedTest(e) => {
                let result =
                    if extendedtests::eval_extended_test_expr(e, pipeline_context.shell).await? {
                        0
                    } else {
                        1
                    };
                Ok(CommandSpawnResult::ImmediateExit(result))
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
            ast::CompoundCommand::BraceGroup(ast::BraceGroupCommand(g)) => {
                g.execute(shell, params).await
            }
            ast::CompoundCommand::Subshell(ast::SubshellCommand(s)) => {
                // Clone off a new subshell, and run the body of the subshell there.
                let mut subshell = shell.clone();
                s.execute(&mut subshell, params).await
            }
            ast::CompoundCommand::ForClause(f) => f.execute(shell, params).await,
            ast::CompoundCommand::CaseClause(c) => c.execute(shell, params).await,
            ast::CompoundCommand::IfClause(i) => i.execute(shell, params).await,
            ast::CompoundCommand::WhileClause(w) => {
                (WhileOrUntil::While, w).execute(shell, params).await
            }
            ast::CompoundCommand::UntilClause(u) => {
                (WhileOrUntil::Until, u).execute(shell, params).await
            }
            ast::CompoundCommand::Arithmetic(a) => a.execute(shell, params).await,
            ast::CompoundCommand::ArithmeticForClause(a) => a.execute(shell, params).await,
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

        if let Some(unexpanded_values) = &self.values {
            // Expand all values, with splitting enabled.
            let mut expanded_values = vec![];
            for value in unexpanded_values {
                let mut expanded = expansion::full_expand_and_split_word(shell, value).await?;
                expanded_values.append(&mut expanded);
            }

            for value in expanded_values {
                if shell.options.print_commands_and_arguments {
                    shell.trace_command(std::format!(
                        "for {} in {}",
                        self.variable_name,
                        unexpanded_values.iter().join(" ")
                    ))?;
                }

                // Update the variable.
                shell.env.update_or_add(
                    &self.variable_name,
                    ShellValueLiteral::Scalar(value),
                    |_| Ok(()),
                    EnvironmentLookup::Anywhere,
                    EnvironmentScope::Global,
                )?;

                result = self.body.0.execute(shell, params).await?;
                if result.return_from_function_or_script {
                    break;
                }

                if let Some(continue_count) = &result.continue_loop {
                    if *continue_count > 0 {
                        return error::unimp("continue with count > 0");
                    }

                    result.continue_loop = None;
                }
                if let Some(break_count) = &result.break_loop {
                    if *break_count == 0 {
                        result.break_loop = None;
                    } else {
                        result.break_loop = Some(*break_count - 1);
                    }
                    break;
                }
            }
        }

        shell.last_exit_status = result.exit_code;
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
            shell.trace_command(std::format!("case {} in", &self.value))?;
        }

        let expanded_value = expansion::basic_expand_word(shell, &self.value).await?;
        let mut result: ExecutionResult = ExecutionResult::success();
        let mut force_execute_next_case = false;

        for case in &self.cases {
            if force_execute_next_case {
                force_execute_next_case = false;
            } else {
                let mut matches = false;
                for pattern in &case.patterns {
                    let expanded_pattern = expansion::basic_expand_pattern(shell, pattern)
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

            match case.post_action {
                ast::CaseItemPostAction::ExitCase => break,
                ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem => {
                    force_execute_next_case = true;
                }
                ast::CaseItemPostAction::ContinueEvaluatingCases => (),
            }
        }

        shell.last_exit_status = result.exit_code;

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

        if condition.is_success() {
            return self.then.execute(shell, params).await;
        }

        if let Some(elses) = &self.elses {
            for else_clause in elses {
                match &else_clause.condition {
                    Some(else_condition) => {
                        let else_condition_result = else_condition.execute(shell, params).await?;
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
        shell.last_exit_status = result.exit_code;

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
        let test_condition = &self.1 .0;
        let body = &self.1 .1;

        let mut result = ExecutionResult::success();

        loop {
            let condition_result = test_condition.execute(shell, params).await?;

            if condition_result.is_success() != is_while {
                break;
            }

            if condition_result.return_from_function_or_script {
                break;
            }

            result = body.0.execute(shell, params).await?;
            if result.return_from_function_or_script {
                break;
            }

            if let Some(continue_count) = &result.continue_loop {
                if *continue_count > 0 {
                    return error::unimp("continue with count > 0");
                }

                result.continue_loop = None;
            }
            if let Some(break_count) = &result.break_loop {
                if *break_count == 0 {
                    result.break_loop = None;
                } else {
                    result.break_loop = Some(*break_count - 1);
                }
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
        _params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let value = self.expr.eval(shell, true).await?;
        let result = if value != 0 {
            ExecutionResult::success()
        } else {
            ExecutionResult::new(1)
        };

        shell.last_exit_status = result.exit_code;

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
            initializer.eval(shell, true).await?;
        }

        loop {
            if let Some(condition) = &self.condition {
                if condition.eval(shell, true).await? == 0 {
                    break;
                }
            }

            result = self.body.0.execute(shell, params).await?;
            if result.return_from_function_or_script {
                break;
            }

            if let Some(continue_count) = &result.continue_loop {
                if *continue_count > 0 {
                    return error::unimp("continue with count > 0");
                }

                result.continue_loop = None;
            }
            if let Some(break_count) = &result.break_loop {
                if *break_count == 0 {
                    result.break_loop = None;
                } else {
                    result.break_loop = Some(*break_count - 1);
                }
                break;
            }

            if let Some(updater) = &self.updater {
                updater.eval(shell, true).await?;
            }
        }

        shell.last_exit_status = result.exit_code;
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
        shell
            .funcs
            .update(self.fname.clone(), Arc::new(self.clone()));

        let result = ExecutionResult::success();
        shell.last_exit_status = result.exit_code;

        Ok(result)
    }
}

#[async_trait::async_trait]
impl ExecuteInPipeline for ast::SimpleCommand {
    #[allow(clippy::too_many_lines)] // TODO: refactor this function
    async fn execute_in_pipeline(
        &self,
        context: &mut PipelineExecutionContext,
    ) -> Result<CommandSpawnResult, error::Error> {
        let default_prefix = ast::CommandPrefix::default();
        let prefix_items = self.prefix.as_ref().unwrap_or(&default_prefix);

        let default_suffix = ast::CommandSuffix::default();
        let suffix_items = self.suffix.as_ref().unwrap_or(&default_suffix);

        let mut cmd_name_items = vec![];
        if let Some(cmd_name) = &self.word_or_name {
            cmd_name_items.push(CommandPrefixOrSuffixItem::Word(cmd_name.clone()));
        }

        let mut params = context.params.clone();
        let mut assignments = vec![];
        let mut args: Vec<CommandArg> = vec![];
        let mut invoking_declaration_builtin = false;

        // Set up pipelining.
        setup_pipeline_redirection(&mut params.open_files, context)?;

        for item in prefix_items
            .0
            .iter()
            .chain(cmd_name_items.iter())
            .chain(suffix_items.0.iter())
        {
            match item {
                CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                    if setup_redirect(&mut params.open_files, context.shell, redirect)
                        .await?
                        .is_none()
                    {
                        // Something went wrong.
                        return Ok(CommandSpawnResult::ImmediateExit(1));
                    }
                }
                CommandPrefixOrSuffixItem::ProcessSubstitution(kind, subshell_command) => {
                    let (installed_fd_num, substitution_file) = setup_process_substitution(
                        &mut params.open_files,
                        context.shell,
                        kind,
                        subshell_command,
                    )?;

                    params
                        .open_files
                        .files
                        .insert(installed_fd_num, substitution_file);

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
                        if invoking_declaration_builtin {
                            // This looks like an assignment, and the command being invoked is a
                            // well-known builtin that takes arguments that need to function like
                            // assignments (but which are processed by the builtin).
                            let expanded = expand_assignment(context.shell, assignment).await?;
                            args.push(CommandArg::Assignment(expanded));
                        } else {
                            // This *looks* like an assignment, but it's really a string we should
                            // fully treat as a regular looking
                            // argument.
                            let mut next_args =
                                expansion::full_expand_and_split_word(context.shell, word)
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
                        expansion::full_expand_and_split_word(context.shell, arg).await?;

                    if args.is_empty() {
                        if let Some(cmd_name) = next_args.first() {
                            if let Some(alias_value) = context.shell.aliases.get(cmd_name.as_str())
                            {
                                //
                                // TODO: This is a total hack; aliases are supposed to be handled
                                // much earlier in the process.
                                //
                                let mut alias_pieces: Vec<_> = alias_value
                                    .split_ascii_whitespace()
                                    .map(|i| i.to_owned())
                                    .collect();

                                next_args.remove(0);
                                alias_pieces.append(&mut next_args);

                                next_args = alias_pieces;
                            }

                            // Check if we're going to be invoking a special declaration builtin.
                            // That will change how we parse and process
                            // args.
                            if context
                                .shell
                                .builtins
                                .get(next_args[0].as_str())
                                .is_some_and(|r| r.declaration_builtin)
                            {
                                invoking_declaration_builtin = true;
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
            // Push a new ephemeral environment scope for the duration of the command. We'll
            // set command-scoped variable assignments after doing so, and revert them before
            // returning.
            context.shell.env.push_scope(EnvironmentScope::Command);
            for assignment in &assignments {
                // Ensure it's tagged as exported and created in the command scope.
                apply_assignment(
                    assignment,
                    context.shell,
                    true,
                    Some(EnvironmentScope::Command),
                    EnvironmentScope::Command,
                )
                .await?;
            }

            if context.shell.options.print_commands_and_arguments {
                context
                    .shell
                    .trace_command(args.iter().map(|arg| arg.quote_for_tracing()).join(" "))?;
            }

            // TODO: This is adding more complexity here; should be factored out into an appropriate
            // helper.
            if context.shell.traps.handler_depth == 0 {
                let debug_trap_handler = context
                    .shell
                    .traps
                    .handlers
                    .get(&traps::TrapSignal::Debug)
                    .cloned();
                if let Some(debug_trap_handler) = debug_trap_handler {
                    // TODO: Confirm whether trap handlers should be executed in the same process
                    // group.
                    let handler_params = ExecutionParameters {
                        open_files: params.open_files.clone(),
                        process_group_policy: ProcessGroupPolicy::SameProcessGroup,
                    };

                    let full_cmd = args.iter().map(|arg| arg.to_string()).join(" ");

                    // TODO: This shouldn't *just* be set in a trap situation.
                    context.shell.env.update_or_add(
                        "BASH_COMMAND",
                        ShellValueLiteral::Scalar(full_cmd),
                        |_| Ok(()),
                        EnvironmentLookup::Anywhere,
                        EnvironmentScope::Global,
                    )?;

                    context.shell.traps.handler_depth += 1;

                    // TODO: Discard result?
                    let _ = context
                        .shell
                        .run_string(debug_trap_handler, &handler_params)
                        .await?;

                    context.shell.traps.handler_depth -= 1;
                }
            }

            let cmd_context = commands::ExecutionContext {
                shell: context.shell,
                command_name: cmd_name,
                params,
            };

            // Execute.
            let execution_result = commands::execute(
                cmd_context,
                &mut context.process_group_id,
                args,
                true, /* use functions? */
            )
            .await;

            // Pop off that ephemeral environment scope.
            // TODO: jobs: do we need to move self back to foreground on error here?
            context.shell.env.pop_scope(EnvironmentScope::Command)?;

            execution_result
        } else {
            // Reset last status.
            context.shell.last_exit_status = 0;

            // No command to run; assignments must be applied to this shell.
            for assignment in assignments {
                apply_assignment(
                    assignment,
                    context.shell,
                    false,
                    None,
                    EnvironmentScope::Global,
                )
                .await?;
            }

            // Return the last exit status we have; in some cases, an expansion
            // might result in a non-zero exit status stored in the shell.
            Ok(CommandSpawnResult::ImmediateExit(
                context.shell.last_exit_status,
            ))
        }
    }
}

async fn expand_assignment(
    shell: &mut Shell,
    assignment: &ast::Assignment,
) -> Result<ast::Assignment, error::Error> {
    let value = expand_assignment_value(shell, &assignment.value).await?;
    Ok(ast::Assignment {
        name: basic_expand_assignment_name(shell, &assignment.name).await?,
        value,
        append: assignment.append,
    })
}

async fn basic_expand_assignment_name(
    shell: &mut Shell,
    name: &ast::AssignmentName,
) -> Result<ast::AssignmentName, error::Error> {
    match name {
        ast::AssignmentName::VariableName(name) => {
            let expanded = expansion::basic_expand_str(shell, name).await?;
            Ok(ast::AssignmentName::VariableName(expanded))
        }
        ast::AssignmentName::ArrayElementName(name, index) => {
            let expanded_name = expansion::basic_expand_str(shell, name).await?;
            let expanded_index = expansion::basic_expand_str(shell, index).await?;
            Ok(ast::AssignmentName::ArrayElementName(
                expanded_name,
                expanded_index,
            ))
        }
    }
}

async fn expand_assignment_value(
    shell: &mut Shell,
    value: &ast::AssignmentValue,
) -> Result<ast::AssignmentValue, error::Error> {
    let expanded = match value {
        ast::AssignmentValue::Scalar(s) => {
            let expanded_word = expansion::basic_expand_word(shell, s).await?;
            ast::AssignmentValue::Scalar(ast::Word {
                value: expanded_word,
            })
        }
        ast::AssignmentValue::Array(arr) => {
            let mut expanded_values = vec![];
            for (key, value) in arr {
                if let Some(k) = key {
                    let expanded_key = expansion::basic_expand_word(shell, k).await?.into();
                    let expanded_value = expansion::basic_expand_word(shell, value).await?.into();
                    expanded_values.push((Some(expanded_key), expanded_value));
                } else {
                    let split_expanded_value =
                        expansion::full_expand_and_split_word(shell, value).await?;
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

#[allow(clippy::too_many_lines)]
async fn apply_assignment(
    assignment: &ast::Assignment,
    shell: &mut Shell,
    export: bool,
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
            let expanded = expansion::basic_expand_str(shell, index).await?;
            array_index = Some(expanded);
            name
        }
    };

    // Expand the values.
    let new_value = match &assignment.value {
        ast::AssignmentValue::Scalar(unexpanded_value) => {
            let value = expansion::basic_expand_word(shell, unexpanded_value).await?;
            ShellValueLiteral::Scalar(value)
        }
        ast::AssignmentValue::Array(unexpanded_values) => {
            let mut elements = vec![];
            for (unexpanded_key, unexpanded_value) in unexpanded_values {
                let key = match unexpanded_key {
                    Some(unexpanded_key) => {
                        Some(expansion::basic_expand_word(shell, unexpanded_key).await?)
                    }
                    None => None,
                };

                if key.is_some() {
                    let value = expansion::basic_expand_word(shell, unexpanded_value).await?;
                    elements.push((key, value));
                } else {
                    let values =
                        expansion::full_expand_and_split_word(shell, unexpanded_value).await?;
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
        shell.trace_command(std::format!("{}{op}{new_value}", assignment.name))?;
    }

    // See if we need to eval an array index.
    if let Some(idx) = &array_index {
        let will_be_indexed_array = if let Some((_, existing_value)) =
            shell.env.get(variable_name.as_str())
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
                ast::UnexpandedArithmeticExpr { value: idx.clone() }
                    .eval(shell, false)
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
                ShellValue::indexed_array_from_literals(ArrayLiteral(vec![(Some(array_index), s)]))?
            }
            ShellValueLiteral::Array(_) => {
                return error::unimp("cannot assign list to array member");
            }
        }
    } else {
        match new_value {
            ShellValueLiteral::Scalar(s) => ShellValue::String(s),
            ShellValueLiteral::Array(values) => ShellValue::indexed_array_from_literals(values)?,
        }
    };

    let mut new_var = ShellVariable::new(new_value);

    if export {
        new_var.export();
    }

    shell.env.add(variable_name, new_var, creation_scope)
}

fn setup_pipeline_redirection(
    open_files: &mut OpenFiles,
    context: &mut PipelineExecutionContext<'_>,
) -> Result<(), error::Error> {
    if context.current_pipeline_index > 0 {
        // Find the stdout from the preceding process.
        if let Some(preceding_output_reader) = context.output_pipes.pop() {
            // Set up stdin of this process to take stdout of the preceding process.
            open_files
                .files
                .insert(0, OpenFile::PipeReader(preceding_output_reader));
        } else {
            open_files.files.insert(0, OpenFile::Null);
        }
    }

    // If this is a non-last command in a multi-command, then we need to arrange to redirect output
    // to a pipe that we can read later.
    if context.pipeline_len > 1 && context.current_pipeline_index < context.pipeline_len - 1 {
        // Set up stdout of this process to go to stdin of the succeeding process.
        let (reader, writer) = sys::pipes::pipe()?;
        context.output_pipes.push(reader);
        open_files.files.insert(1, OpenFile::PipeWriter(writer));
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn setup_redirect(
    open_files: &'_ mut OpenFiles,
    shell: &mut Shell,
    redirect: &ast::IoRedirect,
) -> Result<Option<u32>, error::Error> {
    match redirect {
        ast::IoRedirect::OutputAndError(f, append) => {
            let mut expanded_fields = expansion::full_expand_and_split_word(shell, f).await?;
            if expanded_fields.len() != 1 {
                return Err(error::Error::InvalidRedirection);
            }

            let expanded_file_path: PathBuf =
                shell.get_absolute_path(Path::new(expanded_fields.remove(0).as_str()));

            let opened_file = std::fs::File::options()
                .create(true)
                .write(true)
                .truncate(!*append)
                .append(*append)
                .open(expanded_file_path.as_path())
                .map_err(|err| {
                    error::Error::RedirectionFailure(
                        expanded_file_path.to_string_lossy().to_string(),
                        err,
                    )
                })?;

            let stdout_file = OpenFile::File(opened_file);
            let stderr_file = stdout_file.try_dup()?;

            open_files.files.insert(1, stdout_file);
            open_files.files.insert(2, stderr_file);

            Ok(Some(1))
        }
        ast::IoRedirect::File(specified_fd_num, kind, target) => {
            let fd_num;
            let target_file;
            match target {
                ast::IoFileRedirectTarget::Filename(f) => {
                    let mut options = std::fs::File::options();

                    let default_fd_if_unspecified = get_default_fd_for_redirect_kind(kind);
                    match kind {
                        ast::IoFileRedirectKind::Read => {
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::Write => {
                            // TODO: honor noclobber options
                            options.create(true);
                            options.write(true);
                            options.truncate(true);
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

                    fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    let mut expanded_fields =
                        expansion::full_expand_and_split_word(shell, f).await?;

                    if expanded_fields.len() != 1 {
                        return Err(error::Error::InvalidRedirection);
                    }

                    let expanded_file_path: PathBuf =
                        shell.get_absolute_path(Path::new(expanded_fields.remove(0).as_str()));

                    let opened_file =
                        options.open(expanded_file_path.as_path()).map_err(|err| {
                            error::Error::RedirectionFailure(
                                expanded_file_path.to_string_lossy().to_string(),
                                err,
                            )
                        })?;
                    target_file = OpenFile::File(opened_file);
                }
                ast::IoFileRedirectTarget::Fd(fd) => {
                    let default_fd_if_unspecified = match kind {
                        ast::IoFileRedirectKind::DuplicateInput => 0,
                        ast::IoFileRedirectKind::DuplicateOutput => 1,
                        _ => {
                            return error::unimp("unexpected redirect kind");
                        }
                    };

                    fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    if let Some(f) = open_files.files.get(fd) {
                        target_file = f.try_dup()?;
                    } else {
                        tracing::error!("{}: Bad file descriptor", fd);
                        return Ok(None);
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
                                open_files,
                                shell,
                                substitution_kind,
                                subshell_cmd,
                            )?;

                            target_file = substitution_file.try_dup()?;
                            open_files.files.insert(substitution_fd, substitution_file);

                            fd_num = specified_fd_num
                                .unwrap_or_else(|| get_default_fd_for_redirect_kind(kind));
                        }
                        _ => return error::unimp("invalid process substitution"),
                    }
                }
            }

            open_files.files.insert(fd_num, target_file);
            Ok(Some(fd_num))
        }
        ast::IoRedirect::HereDocument(fd_num, io_here) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            // Expand if required.
            let io_here_doc = if io_here.requires_expansion {
                expansion::basic_expand_word(shell, &io_here.doc).await?
            } else {
                io_here.doc.flatten()
            };

            let f = setup_open_file_with_contents(io_here_doc.as_str())?;

            open_files.files.insert(fd_num, f);
            Ok(Some(fd_num))
        }
        ast::IoRedirect::HereString(fd_num, word) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            let mut expanded_word = expansion::basic_expand_word(shell, word).await?;
            expanded_word.push('\n');

            let f = setup_open_file_with_contents(expanded_word.as_str())?;

            open_files.files.insert(fd_num, f);
            Ok(Some(fd_num))
        }
    }
}

fn get_default_fd_for_redirect_kind(kind: &ast::IoFileRedirectKind) -> u32 {
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
    open_files: &mut OpenFiles,
    shell: &mut Shell,
    kind: &ast::ProcessSubstitutionKind,
    subshell_cmd: &ast::SubshellCommand,
) -> Result<(u32, OpenFile), error::Error> {
    // TODO: Don't execute synchronously!
    // Execute in a subshell.
    let mut subshell = shell.clone();

    // Set up pipe so we can connect to the command.
    let (reader, writer) = sys::pipes::pipe()?;

    let target_file = match kind {
        ast::ProcessSubstitutionKind::Read => {
            subshell
                .open_files
                .files
                .insert(1, openfiles::OpenFile::PipeWriter(writer));
            OpenFile::PipeReader(reader)
        }
        ast::ProcessSubstitutionKind::Write => {
            subshell
                .open_files
                .files
                .insert(0, openfiles::OpenFile::PipeReader(reader));
            OpenFile::PipeWriter(writer)
        }
    };

    let exec_params = ExecutionParameters {
        open_files: subshell.open_files.clone(),
        process_group_policy: ProcessGroupPolicy::SameProcessGroup,
    };

    // Asynchronously spawn off the subshell; we intentionally don't block on its
    // completion.
    let subshell_cmd = subshell_cmd.to_owned();
    tokio::spawn(async move {
        // Intentionally ignore the result of the subshell command.
        let _ = subshell_cmd.0.execute(&mut subshell, &exec_params).await;
    });

    // Starting at 63 (a.k.a. 64-1)--and decrementing--look for an
    // available fd.
    let mut candidate_fd_num = 63;
    while open_files.files.contains_key(&candidate_fd_num) {
        candidate_fd_num -= 1;
        if candidate_fd_num == 0 {
            return error::unimp("no available file descriptors");
        }
    }

    Ok((candidate_fd_num, target_file))
}

#[allow(unused_variables)]
fn setup_open_file_with_contents(contents: &str) -> Result<OpenFile, error::Error> {
    let (reader, mut writer) = sys::pipes::pipe()?;

    let bytes = contents.as_bytes();
    let len = i32::try_from(bytes.len())?;

    #[cfg(target_os = "linux")]
    nix::fcntl::fcntl(
        reader.as_fd().as_raw_fd(),
        nix::fcntl::FcntlArg::F_SETPIPE_SZ(len),
    )?;

    writer.write_all(bytes)?;
    drop(writer);

    Ok(OpenFile::PipeReader(reader))
}
