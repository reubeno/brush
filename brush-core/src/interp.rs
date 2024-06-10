use brush_parser::ast::{self, CommandPrefixOrSuffixItem};
use itertools::Itertools;
use std::collections::VecDeque;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process;

use crate::arithmetic::ExpandAndEvaluate;
use crate::commands::{self, CommandArg};
use crate::env::{EnvironmentLookup, EnvironmentScope};
use crate::openfiles::{OpenFile, OpenFiles};
use crate::shell::Shell;
use crate::variables::{
    ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
};
use crate::{builtin, error, expansion, extendedtests, jobs, openfiles, traps};

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
}

/// Represents the result of spawning a command.
pub(crate) enum SpawnResult {
    /// The child process was spawned.
    SpawnedChild(process::Child),
    /// The command immediatedly exited with the given numeric exit code.
    ImmediateExit(u8),
    /// The shell should exit after this command, yielding the given numeric exit code.
    ExitShell(u8),
    /// The shell should return from the current function or script, yielding the given numeric exit code.
    ReturnFromFunctionOrScript(u8),
    /// The shell should break out of the containing loop, identified by the given depth count.
    BreakLoop(u8),
    /// The shell should continue the containing loop, identified by the given depth count.
    ContinueLoop(u8),
}

/// Encapsulates the context of execution in a command pipeline.
struct PipelineExecutionContext<'a> {
    /// The shell in which the command is being executed.
    shell: &'a mut Shell,

    current_pipeline_index: usize,
    pipeline_len: usize,
    output_pipes: Vec<os_pipe::PipeReader>,

    params: ExecutionParameters,
}

/// Parameters for execution.
#[derive(Clone, Default)]
pub struct ExecutionParameters {
    /// The open files tracked by the current context.
    pub open_files: openfiles::OpenFiles,
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
    ) -> Result<SpawnResult, error::Error>;
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
        VecDeque::from([join_handle]),
        vec![],
        ao_list.to_string(),
        jobs::JobState::Running,
    ));

    job
}

// async fn spawn_ao_list_in_child<'a>(
//     ao_list: &ast::AndOrList,
//     shell: &'a mut Shell,
//     params: &ExecutionParameters,
// ) -> Result<&'a jobs::Job, error::Error> {
//     let fork_result = unsafe { nix::unistd::fork() }?;

//     #[allow(clippy::cast_lossless)]
//     #[allow(clippy::cast_sign_loss)]
//     match fork_result {
//         nix::unistd::ForkResult::Parent { child } => {
//             let join_handle = tokio::spawn(async move {
//                 let wait_status = nix::sys::wait::waitid(
//                     nix::sys::wait::Id::Pid(child),
//                     nix::sys::wait::WaitPidFlag::WEXITED,
//                 )?;

//                 #[allow(clippy::cast_possible_truncation)]
//                 if let nix::sys::wait::WaitStatus::Exited(_, code) = wait_status {
//                     Ok(ExecutionResult::new(code as u8))
//                 } else {
//                     Ok(ExecutionResult::new(1))
//                 }
//             });

//             let job = shell.jobs.add_as_current(jobs::Job::new(
//                 VecDeque::from([join_handle]),
//                 vec![child.as_raw() as u32],
//                 ao_list.to_string(),
//                 jobs::JobState::Running,
//             ));

//             Ok(job)
//         }
//         nix::unistd::ForkResult::Child => {
//             if nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
//                 .is_err()
//             {
//                 std::process::exit(1);
//             }

//             let result = ao_list.execute(shell, params).await?;
//             std::process::exit(result.exit_code as i32);
//         }
//     }
// }

#[async_trait::async_trait]
impl Execute for ast::AndOrList {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut result = self.first.execute(shell, params).await?;

        for next_ao in &self.additional {
            if result.exit_shell || result.return_from_function_or_script {
                break;
            }

            // Check for continue/break
            if result.continue_loop.is_some() || result.break_loop.is_some() {
                return error::unimp("continue || break in and-or list");
            }

            let (is_and, pipeline) = match next_ao {
                ast::AndOr::And(p) => (true, p),
                ast::AndOr::Or(p) => (false, p),
            };

            if is_and {
                if !result.is_success() {
                    break;
                }
            } else if result.is_success() {
                break;
            }

            result = pipeline.execute(shell, params).await?;
        }

        Ok(result)
    }
}

enum ExecuteWaitResult {
    WaitCompleted(std::process::Output),
    Sigtstp,
}

#[allow(clippy::too_many_lines)] // TODO: refactor this function
#[async_trait::async_trait]
impl Execute for ast::Pipeline {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        let mut pipeline_context = PipelineExecutionContext {
            shell,
            current_pipeline_index: 0,
            pipeline_len: self.seq.len(),
            output_pipes: vec![],
            params: params.clone(),
        };

        let mut spawn_results = VecDeque::new();

        for command in &self.seq {
            let spawn_result = command.execute_in_pipeline(&mut pipeline_context).await?;
            spawn_results.push_back(spawn_result);
            pipeline_context.current_pipeline_index += 1;
        }

        let mut result = ExecutionResult::success();

        const SIGTSTP: std::os::raw::c_int = 20;

        #[cfg(unix)]
        let mut sigtstp =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::from_raw(SIGTSTP))?;
        #[cfg(not(unix))]
        let sigtstp = FakeSignal::new();

        let mut stopped: VecDeque<jobs::JobJoinHandle> = VecDeque::new();
        let mut pids = vec![];
        while let Some(child) = spawn_results.pop_front() {
            match child {
                SpawnResult::SpawnedChild(child) => {
                    if let Some(pid) = child.id() {
                        pids.push(pid);
                    }

                    let mut child_future = Box::pin(child.wait_with_output());

                    // Wait for the process to exit or for a relevant signal, whichever happens first.
                    // TODO: Figure out how to detect a SIGSTOP'd process.
                    let wait_result = if stopped.is_empty() {
                        loop {
                            tokio::select! {
                                output = &mut child_future => {
                                    break ExecuteWaitResult::WaitCompleted(output?)
                                },
                                _ = sigtstp.recv() => {
                                    break ExecuteWaitResult::Sigtstp
                                },
                                _ = tokio::signal::ctrl_c() => {
                                    // SIGINT got thrown. Handle it and continue looping. The child should
                                    // have received it as well, and either handled it or ended up getting
                                    // terminated (in which case we'll see the child exit).
                                },
                            }
                        }
                    } else {
                        ExecuteWaitResult::Sigtstp
                    };

                    match wait_result {
                        ExecuteWaitResult::WaitCompleted(output) => {
                            result = ExecutionResult::from(output);
                        }
                        #[allow(clippy::cast_possible_truncation)]
                        ExecuteWaitResult::Sigtstp => {
                            stopped.push_back(tokio::spawn(async move {
                                child_future
                                    .await
                                    .map(ExecutionResult::from)
                                    .map_err(|err| err.into())
                            }));

                            result = ExecutionResult::new(128 + SIGTSTP as u8);
                        }
                    }
                }
                SpawnResult::ImmediateExit(exit_code) => {
                    result = ExecutionResult::new(exit_code);
                }
                SpawnResult::ExitShell(exit_code) => {
                    result = ExecutionResult {
                        exit_code,
                        exit_shell: true,
                        ..ExecutionResult::default()
                    };
                }
                SpawnResult::ReturnFromFunctionOrScript(exit_code) => {
                    result = ExecutionResult {
                        exit_code,
                        return_from_function_or_script: true,
                        ..ExecutionResult::default()
                    }
                }
                SpawnResult::BreakLoop(count) => {
                    result = ExecutionResult {
                        exit_code: 0,
                        break_loop: Some(count),
                        ..ExecutionResult::default()
                    }
                }
                SpawnResult::ContinueLoop(count) => {
                    result = ExecutionResult {
                        exit_code: 0,
                        continue_loop: Some(count),
                        ..ExecutionResult::default()
                    }
                }
            }

            shell.last_exit_status = result.exit_code;
        }

        if !stopped.is_empty() {
            let job = shell.jobs.add_as_current(jobs::Job::new(
                stopped,
                pids,
                self.to_string(),
                jobs::JobState::Stopped,
            ));

            let formatted = job.to_string();

            // N.B. We use the '\r' to overwrite any ^Z output.
            writeln!(shell.stderr(), "\r{formatted}")?;
        }

        if self.bang {
            result.exit_code = if result.exit_code == 0 { 1 } else { 0 };
        }

        shell.last_exit_status = result.exit_code;

        Ok(result)
    }
}

#[async_trait::async_trait]
impl ExecuteInPipeline for ast::Command {
    async fn execute_in_pipeline(
        &self,
        pipeline_context: &mut PipelineExecutionContext,
    ) -> Result<SpawnResult, error::Error> {
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
                    Ok(SpawnResult::ExitShell(result.exit_code))
                } else if result.return_from_function_or_script {
                    Ok(SpawnResult::ReturnFromFunctionOrScript(result.exit_code))
                } else if let Some(count) = result.break_loop {
                    Ok(SpawnResult::BreakLoop(count))
                } else if let Some(count) = result.continue_loop {
                    Ok(SpawnResult::ContinueLoop(count))
                } else {
                    Ok(SpawnResult::ImmediateExit(result.exit_code))
                }
            }
            ast::Command::Function(func) => {
                let result = func
                    .execute(pipeline_context.shell, &pipeline_context.params)
                    .await?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
            ast::Command::ExtendedTest(e) => {
                let result =
                    if extendedtests::eval_extended_test_expr(e, pipeline_context.shell).await? {
                        0
                    } else {
                        1
                    };
                Ok(SpawnResult::ImmediateExit(result))
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

        for case in &self.cases {
            let mut matches = false;

            for pattern in &case.patterns {
                let expanded_pattern = expansion::basic_expand_pattern(shell, pattern).await?;
                if expanded_pattern
                    .exactly_matches(expanded_value.as_str(), shell.options.extended_globbing)?
                {
                    matches = true;
                    break;
                }
            }

            if matches {
                if let Some(case_cmd) = &case.cmd {
                    return case_cmd.execute(shell, params).await;
                } else {
                    break;
                }
            }
        }

        let result = ExecutionResult::success();
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
    ) -> Result<SpawnResult, error::Error> {
        let default_prefix = ast::CommandPrefix::default();
        let prefix_items = self.prefix.as_ref().unwrap_or(&default_prefix);

        let default_suffix = ast::CommandSuffix::default();
        let suffix_items = self.suffix.as_ref().unwrap_or(&default_suffix);

        let mut cmd_name_items = vec![];
        if let Some(cmd_name) = &self.word_or_name {
            cmd_name_items.push(CommandPrefixOrSuffixItem::Word(cmd_name.clone()));
        }

        let mut open_files = context.params.open_files.clone();
        let mut assignments = vec![];
        let mut args: Vec<CommandArg> = vec![];
        let mut invoking_declaration_builtin = false;

        // Set up pipelining.
        setup_pipeline_redirection(&mut open_files, context)?;

        for item in prefix_items
            .0
            .iter()
            .chain(cmd_name_items.iter())
            .chain(suffix_items.0.iter())
        {
            match item {
                CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                    if let Some(installed_fd_num) =
                        setup_redirect(&mut open_files, context.shell, redirect).await?
                    {
                        if matches!(
                            redirect,
                            ast::IoRedirect::File(
                                _,
                                _,
                                ast::IoFileRedirectTarget::ProcessSubstitution(_)
                            )
                        ) {
                            args.push(CommandArg::String(std::format!(
                                "/dev/fd/{installed_fd_num}"
                            )));
                        }
                    } else {
                        // Something went wrong.
                        return Ok(SpawnResult::ImmediateExit(1));
                    }
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
                            let expanded =
                                basic_expand_assignment(context.shell, assignment).await?;
                            args.push(CommandArg::Assignment(expanded));
                        } else {
                            // This *looks* like an assignment, but it's really a string we should fully
                            // treat as a regular looking argument.
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

                            // Check if we're going to be invoking a special declaration builtin. That will
                            // change how we parse and process args.
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
                    .trace_command(args.iter().map(|arg| arg.to_string()).join(" "))?;
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
                    let params = ExecutionParameters {
                        open_files: open_files.clone(),
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
                        .run_string(debug_trap_handler, &params)
                        .await?;

                    context.shell.traps.handler_depth -= 1;
                }
            }

            let cmd_context = commands::ExecutionContext {
                shell: context.shell,
                command_name: cmd_name,
                open_files,
            };

            let execution_result = if !cmd_context.command_name.contains(std::path::MAIN_SEPARATOR)
            {
                let mut builtin = cmd_context
                    .shell
                    .builtins
                    .get(&cmd_context.command_name)
                    .cloned();

                // Ignore the builtin if it's marked as disabled.
                if builtin.as_ref().is_some_and(|b| b.disabled) {
                    builtin = None;
                }

                if builtin
                    .as_ref()
                    .is_some_and(|r| !r.disabled && r.special_builtin)
                {
                    execute_builtin_command(&builtin.unwrap(), cmd_context, args).await
                } else if let Some(func_reg) = cmd_context
                    .shell
                    .funcs
                    .get(cmd_context.command_name.as_str())
                {
                    // Strip the function name off args.
                    invoke_shell_function(func_reg.definition.clone(), cmd_context, &args[1..])
                        .await
                } else if let Some(builtin) = builtin {
                    execute_builtin_command(&builtin, cmd_context, args).await
                } else {
                    // Strip the command name off args.
                    execute_external_command(cmd_context, &args[1..]).await
                }
            } else {
                // Strip the command name off args.
                execute_external_command(cmd_context, &args[1..]).await
            };

            // Pop off that ephemeral environment scope.
            context.shell.env.pop_scope(EnvironmentScope::Command)?;

            execution_result
        } else {
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

            Ok(SpawnResult::ImmediateExit(0))
        }
    }
}

async fn basic_expand_assignment(
    shell: &mut Shell,
    assignment: &ast::Assignment,
) -> Result<ast::Assignment, error::Error> {
    let value = basic_expand_assignment_value(shell, &assignment.value).await?;
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

async fn basic_expand_assignment_value(
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
                let expanded_key = match key {
                    Some(k) => Some(ast::Word {
                        value: expansion::basic_expand_word(shell, k).await?,
                    }),
                    None => None,
                };

                let expanded_value = expansion::basic_expand_word(shell, value).await?;
                expanded_values.push((
                    expanded_key,
                    ast::Word {
                        value: expanded_value,
                    },
                ));
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
    // Figure out if we are trying to assign to a variable or assign to an element of an existing array.
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
        shell.trace_command(std::format!("{}{}{}", assignment.name, op, new_value))?;
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

#[allow(clippy::too_many_lines)]
async fn execute_external_command(
    context: commands::ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<SpawnResult, error::Error> {
    // Filter out the args; we only want strings.
    let mut cmd_args = vec![];
    for arg in args {
        if let CommandArg::String(s) = arg {
            cmd_args.push(s);
        }
    }

    // Compose the std::process::Command that encapsulates what we want to launch.
    let (cmd, stdin_here_doc) = commands::compose_std_command(
        context.shell,
        context.command_name.as_str(),
        context.command_name.as_str(),
        cmd_args.as_slice(),
        context.open_files,
        false, /* empty environment? */
    )?;

    // When tracing is enabled, report.
    tracing::debug!(
        target: "commands",
        "Spawning: {} {}",
        cmd.get_program().to_string_lossy().to_string(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .join(" ")
    );

    // Convert to a tokio::process::Command so we can execute it asynchronously.
    let mut cmd = tokio::process::Command::from(cmd);

    match cmd.spawn() {
        Ok(mut child) => {
            // Special case: handle writing here document, if needed.
            if let Some(doc) = stdin_here_doc {
                if let Some(mut child_stdin) = child.stdin.take() {
                    child_stdin.write_all(doc.as_bytes()).await?;
                }
            }

            Ok(SpawnResult::SpawnedChild(child))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if context.shell.options.sh_mode {
                tracing::error!(
                    "{}: {}: {}: not found",
                    context.shell.shell_name.as_ref().unwrap_or(&String::new()),
                    context.shell.get_current_input_line_number(),
                    context.command_name
                );
            } else {
                tracing::error!("{}: not found", context.command_name);
            }
            Ok(SpawnResult::ImmediateExit(127))
        }
        Err(e) => {
            tracing::error!("error: {}", e);
            Ok(SpawnResult::ImmediateExit(126))
        }
    }
}

async fn execute_builtin_command(
    builtin: &builtin::Registration,
    context: commands::ExecutionContext<'_>,
    args: Vec<CommandArg>,
) -> Result<SpawnResult, error::Error> {
    let exit_code = match (builtin.execute_func)(context, args).await {
        Ok(builtin_result) => match builtin_result.exit_code {
            builtin::ExitCode::Success => 0,
            builtin::ExitCode::InvalidUsage => 2,
            builtin::ExitCode::Unimplemented => 99,
            builtin::ExitCode::Custom(code) => code,
            builtin::ExitCode::ExitShell(code) => return Ok(SpawnResult::ExitShell(code)),
            builtin::ExitCode::ReturnFromFunctionOrScript(code) => {
                return Ok(SpawnResult::ReturnFromFunctionOrScript(code))
            }
            builtin::ExitCode::BreakLoop(count) => return Ok(SpawnResult::BreakLoop(count)),
            builtin::ExitCode::ContinueLoop(count) => return Ok(SpawnResult::ContinueLoop(count)),
        },
        Err(e) => {
            tracing::error!("error: {}", e);
            1
        }
    };

    Ok(SpawnResult::ImmediateExit(exit_code))
}

pub(crate) async fn invoke_shell_function(
    function_definition: Arc<ast::FunctionDefinition>,
    mut context: commands::ExecutionContext<'_>,
    args: &[CommandArg],
) -> Result<SpawnResult, error::Error> {
    let ast::FunctionBody(body, redirects) = &function_definition.body;

    // Apply any redirects specified at function definition-time.
    if let Some(redirects) = redirects {
        for redirect in &redirects.0 {
            setup_redirect(&mut context.open_files, context.shell, redirect).await?;
        }
    }

    // Temporarily replace positional parameters.
    let prior_positional_params = std::mem::take(&mut context.shell.positional_parameters);
    context.shell.positional_parameters = args.iter().map(|a| a.to_string()).collect();

    // Pass through open files.
    let params = ExecutionParameters {
        open_files: context.open_files.clone(),
    };

    // Note that we're going deeper. Once we do this, we need to make sure we don't bail early
    // before "exiting" the function.
    context
        .shell
        .enter_function(context.command_name.as_str(), &function_definition)?;

    // Invoke the function.
    let result = body.execute(context.shell, &params).await;

    // Clean up parameters so any owned files are closed.
    drop(params);

    // We've come back out, reflect it.
    context.shell.leave_function()?;

    // Restore positional parameters.
    context.shell.positional_parameters = prior_positional_params;

    Ok(SpawnResult::ImmediateExit(result?.exit_code))
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
        let (reader, writer) = os_pipe::pipe()?;
        context.output_pipes.push(reader);
        open_files.files.insert(1, OpenFile::PipeWriter(writer));
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn setup_redirect<'a>(
    open_files: &'a mut OpenFiles,
    shell: &mut Shell,
    redirect: &ast::IoRedirect,
) -> Result<Option<u32>, error::Error> {
    match redirect {
        ast::IoRedirect::OutputAndError(f) => {
            let mut expanded_file_path = expansion::full_expand_and_split_word(shell, f).await?;
            if expanded_file_path.len() != 1 {
                return Err(error::Error::InvalidRedirection);
            }

            let expanded_file_path = expanded_file_path.remove(0);

            let opened_file = std::fs::File::options()
                .create(true)
                .write(true)
                .truncate(true)
                .open(expanded_file_path.as_str())
                .map_err(|err| error::Error::RedirectionFailure(expanded_file_path, err))?;

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

                    let default_fd_if_unspecified;
                    match kind {
                        ast::IoFileRedirectKind::Read => {
                            default_fd_if_unspecified = 0;
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::Write => {
                            // TODO: honor noclobber options
                            default_fd_if_unspecified = 1;
                            options.create(true);
                            options.write(true);
                            options.truncate(true);
                        }
                        ast::IoFileRedirectKind::Append => {
                            default_fd_if_unspecified = 1;
                            options.create(true);
                            options.append(true);
                        }
                        ast::IoFileRedirectKind::ReadAndWrite => {
                            default_fd_if_unspecified = 0;
                            options.create(true);
                            options.read(true);
                            options.write(true);
                        }
                        ast::IoFileRedirectKind::Clobber => {
                            default_fd_if_unspecified = 1;
                            options.create(true);
                            options.write(true);
                            options.truncate(true);
                        }
                        ast::IoFileRedirectKind::DuplicateInput => {
                            default_fd_if_unspecified = 0;
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::DuplicateOutput => {
                            default_fd_if_unspecified = 1;
                            options.create(true);
                            options.write(true);
                        }
                    }

                    fd_num = specified_fd_num.unwrap_or(default_fd_if_unspecified);

                    let mut expanded_file_path =
                        expansion::full_expand_and_split_word(shell, f).await?;

                    if expanded_file_path.len() != 1 {
                        return Err(error::Error::InvalidRedirection);
                    }

                    let expanded_file_path = expanded_file_path.remove(0);

                    let opened_file = options
                        .open(expanded_file_path.as_str())
                        .map_err(|err| error::Error::RedirectionFailure(expanded_file_path, err))?;
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
                ast::IoFileRedirectTarget::ProcessSubstitution(ast::SubshellCommand(
                    subshell_cmd,
                )) => {
                    match kind {
                        ast::IoFileRedirectKind::Read | ast::IoFileRedirectKind::Write => {
                            // TODO: Don't execute synchronously!
                            // Execute in a subshell.
                            let mut subshell = shell.clone();

                            // Set up pipe so we can connect to the command.
                            let (reader, writer) = os_pipe::pipe()?;

                            if matches!(kind, ast::IoFileRedirectKind::Read) {
                                subshell
                                    .open_files
                                    .files
                                    .insert(1, openfiles::OpenFile::PipeWriter(writer));
                                target_file = OpenFile::PipeReader(reader);
                            } else {
                                subshell
                                    .open_files
                                    .files
                                    .insert(0, openfiles::OpenFile::PipeReader(reader));
                                target_file = OpenFile::PipeWriter(writer);
                            }

                            let exec_params = ExecutionParameters {
                                open_files: subshell.open_files.clone(),
                            };

                            // TODO: inspect result of execution?
                            let _ = subshell_cmd.execute(&mut subshell, &exec_params).await?;

                            // Make sure the subshell + parameters are closed; among other
                            // things, this ensures they're not holding onto the write end
                            // of the pipe.
                            drop(exec_params);
                            drop(subshell);

                            // Starting at 63 (a.k.a. 64-1)--and decrementing--look for an
                            // available fd.
                            let mut candidate_fd_num = 63;
                            while open_files.files.contains_key(&candidate_fd_num) {
                                candidate_fd_num -= 1;
                                if candidate_fd_num == 0 {
                                    return error::unimp("no available file descriptors");
                                }
                            }

                            fd_num = candidate_fd_num;
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

            // TODO: figure out if we need to expand?
            let io_here_doc = io_here.doc.flatten();

            open_files
                .files
                .insert(fd_num, OpenFile::HereDocument(io_here_doc));
            Ok(Some(fd_num))
        }
        ast::IoRedirect::HereString(fd_num, word) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            let mut expanded_word = expansion::basic_expand_word(shell, word).await?;
            expanded_word.push('\n');

            open_files
                .files
                .insert(fd_num, OpenFile::HereDocument(expanded_word));
            Ok(Some(fd_num))
        }
    }
}

#[cfg(not(unix))]
struct FakeSignal {}

#[cfg(not(unix))]
impl FakeSignal {
    fn new() -> Self {
        Self {}
    }

    async fn recv(&self) -> () {
        futures::future::pending::<()>().await
    }
}
