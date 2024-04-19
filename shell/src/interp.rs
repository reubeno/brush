use std::collections::HashSet;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process;

use anyhow::{Context, Result};
use itertools::Itertools;
use parser::ast::{self, CommandPrefixOrSuffixItem};
use tokio_command_fds::{CommandFdExt, FdMapping};

use crate::arithmetic::Evaluatable;
use crate::commands::CommandArg;
use crate::env::{EnvironmentLookup, EnvironmentScope};
use crate::openfiles::{OpenFile, OpenFiles};
use crate::shell::Shell;
use crate::variables::{
    ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
};
use crate::{builtin, builtins};
use crate::{context, error};
use crate::{expansion, openfiles};
use crate::{extendedtests, patterns};

#[derive(Debug, Default)]
pub struct ExecutionResult {
    pub exit_code: u8,
    pub exit_shell: bool,
    pub return_from_function_or_script: bool,
    pub break_loop: Option<u8>,
    pub continue_loop: Option<u8>,
}

impl ExecutionResult {
    pub fn new(exit_code: u8) -> ExecutionResult {
        ExecutionResult {
            exit_code,
            ..ExecutionResult::default()
        }
    }

    pub fn success() -> ExecutionResult {
        Self::new(0)
    }

    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

pub(crate) enum SpawnResult {
    SpawnedChild(process::Child),
    ImmediateExit(u8),
    ExitShell(u8),
    ReturnFromFunctionOrScript(u8),
    BreakLoop(u8),
    ContinueLoop(u8),
}

struct PipelineExecutionContext<'a> {
    shell: &'a mut Shell,

    current_pipeline_index: usize,
    pipeline_len: usize,

    spawn_results: Vec<SpawnResult>,
    output_pipes: Vec<os_pipe::PipeReader>,

    params: ExecutionParameters,
}

#[derive(Clone, Default)]
pub struct ExecutionParameters {
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
                let background_job = tokio::spawn(execute_ao_list_async(
                    shell.clone(),
                    params.clone(),
                    ao_list.clone(),
                ));

                let job_number = shell.jobs.add(background_job);

                // TODO: Write this to the correct ouptut stream.
                println!("[{job_number}] <pid unknown>");
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

async fn execute_ao_list_async(
    mut shell: Shell,
    params: ExecutionParameters,
    ao_list: ast::AndOrList,
) -> Result<ExecutionResult, error::Error> {
    let background_job = ao_list.execute(&mut shell, &params).await?;
    Ok(background_job)
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

#[async_trait::async_trait]
impl Execute for ast::Pipeline {
    async fn execute(
        &self,
        shell: &mut Shell,
        params: &ExecutionParameters,
    ) -> Result<ExecutionResult, error::Error> {
        //
        // TODO: implement logic deciding when to abort
        //

        let mut pipeline_context = PipelineExecutionContext {
            shell,
            current_pipeline_index: 0,
            pipeline_len: self.seq.len(),
            spawn_results: vec![],
            output_pipes: vec![],
            params: params.clone(),
        };

        for command in &self.seq {
            let spawn_result = command.execute_in_pipeline(&mut pipeline_context).await?;
            pipeline_context.spawn_results.push(spawn_result);
            pipeline_context.current_pipeline_index += 1;
        }

        let mut result = ExecutionResult::success();

        for child in pipeline_context.spawn_results {
            match child {
                SpawnResult::SpawnedChild(child) => {
                    let child_future = child.wait_with_output();
                    tokio::pin!(child_future);

                    // Wait for the process to exit or for interruption, whichever happens first.
                    let output = loop {
                        tokio::select! {
                            output = &mut child_future => {
                                break output?
                            },
                            _ = tokio::signal::ctrl_c() => {
                            },
                        }
                    };

                    let exit_code;

                    #[allow(clippy::cast_sign_loss)]
                    if let Some(code) = output.status.code() {
                        exit_code = (code & 0xFF) as u8;
                    } else if let Some(signal) = output.status.signal() {
                        exit_code = (signal & 0xFF) as u8 + 128;
                    } else {
                        return error::unimp("unhandled process exit");
                    }

                    // TODO: Confirm what to return if it was signaled.
                    result = ExecutionResult::new(exit_code);
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

                // Set up redirects.
                if let Some(redirects) = redirects {
                    for redirect in &redirects.0 {
                        setup_redirect(&mut params.open_files, pipeline_context.shell, redirect)
                            .await?;
                    }
                }

                // TODO: Need to execute in the pipeline.
                let result = compound.execute(pipeline_context.shell, &params).await?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
            ast::Command::Function(func) => {
                // TODO: Need to execute in pipeline.
                let result = func
                    .execute(pipeline_context.shell, &pipeline_context.params)
                    .await?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
            ast::Command::ExtendedTest(e) => {
                let result = if extendedtests::eval_expression(e, pipeline_context.shell).await? {
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
                // Update the variable.
                shell.env.update_or_add(
                    &self.variable_name,
                    ShellValueLiteral::Scalar(value),
                    |_| Ok(()),
                    EnvironmentLookup::Anywhere,
                    EnvironmentScope::Global,
                )?;

                result = self.body.0.execute(shell, params).await?;

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
        let expanded_value = expansion::basic_expand_word(shell, &self.value).await?;
        for case in &self.cases {
            let mut matches = false;

            for pattern in &case.patterns {
                // TODO: Handle quoting's impact on pattern matching.
                let expanded_pattern = expansion::basic_expand_word(shell, pattern).await?;
                if patterns::pattern_matches(expanded_pattern.as_str(), expanded_value.as_str())? {
                    matches = true;
                    break;
                }
            }

            if matches {
                if let Some(case_cmd) = &case.cmd {
                    return case_cmd.execute(shell, params).await;
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

            result = body.0.execute(shell, params).await?;
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
        let value = self.expr.eval(shell).await?;
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
            initializer.eval(shell).await?;
        }

        loop {
            if let Some(condition) = &self.condition {
                if condition.eval(shell).await? == 0 {
                    break;
                }
            }

            result = self.body.0.execute(shell, params).await?;

            if let Some(updater) = &self.updater {
                updater.eval(shell).await?;
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
        shell.funcs.insert(self.fname.clone(), self.clone());

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
                    // TODO: Reevaluate if this is an appropriate place to handle aliases.
                    let mut next_args =
                        expansion::full_expand_and_split_word(context.shell, arg).await?;

                    if args.is_empty() {
                        if let Some(cmd_name) = next_args.first() {
                            if let Some(alias_value) = context.shell.aliases.get(cmd_name.as_str())
                            {
                                //
                                // TODO: This is a total hack.
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
                            if builtins::DECLARATION_BUILTINS.contains(next_args[0].as_str()) {
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
            if context.shell.options.print_commands_and_arguments {
                writeln!(
                    context.shell.stdout(),
                    "+ {}",
                    args.iter().map(|arg| arg.to_string()).join(" ")
                )?;
            }

            // Apply all variable assignments in a child shell object, from which we'll harvest
            // the values to pass through to the child.
            let mut child_pseudo_shell = context.shell.clone();
            let mut assigned_vars = HashSet::new();
            for assignment in &assignments {
                if let ast::AssignmentName::VariableName(name) = &assignment.name {
                    assigned_vars.insert(name.clone());
                }
                apply_assignment(assignment, &mut child_pseudo_shell).await?;
            }

            // Extract assigned variables.
            let env_vars = child_pseudo_shell
                .env
                .iter()
                .filter_map(|(name, var)| {
                    if assigned_vars.contains(name) {
                        Some((name.clone(), var.value().clone()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            let cmd_context = context::CommandExecutionContext {
                shell: context.shell,
                command_name: cmd_name,
                open_files,
            };

            if !cmd_context.command_name.contains('/') {
                let normal_builtin_lookup = if cmd_context.shell.options.sh_mode {
                    |s: &str| builtins::POSIX_ONLY_BUILTINS.get(s)
                } else {
                    |s: &str| builtins::BUILTINS.get(s)
                };

                if let Some(builtin) =
                    builtins::SPECIAL_BUILTINS.get(cmd_context.command_name.as_str())
                {
                    execute_builtin_command(*builtin, cmd_context, args, env_vars).await
                } else if cmd_context
                    .shell
                    .funcs
                    .contains_key(cmd_context.command_name.as_str())
                {
                    // Strip the function name off args.
                    invoke_shell_function(cmd_context, &args[1..], &env_vars).await
                } else if let Some(builtin) =
                    normal_builtin_lookup(cmd_context.command_name.as_str())
                {
                    execute_builtin_command(*builtin, cmd_context, args, env_vars).await
                } else {
                    // Strip the command name off args.
                    execute_external_command(cmd_context, &args[1..], &env_vars).await
                }
            } else {
                // Strip the command name off args.
                execute_external_command(cmd_context, &args[1..], &env_vars).await
            }
        } else {
            // No command to run; assignments must be applied to this shell.
            for assignment in assignments {
                if context.shell.options.print_commands_and_arguments {
                    writeln!(context.shell.stdout(), "+ {assignment}")?;
                }

                apply_assignment(assignment, context.shell).await?;
            }

            Ok(SpawnResult::ImmediateExit(0))
        }
    }
}

#[allow(dead_code)]
async fn basic_expand_assignment(
    shell: &mut Shell,
    assignment: &ast::Assignment,
) -> Result<ast::Assignment> {
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
) -> Result<ast::AssignmentName> {
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
) -> Result<ast::AssignmentValue> {
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

async fn apply_assignment(
    assignment: &ast::Assignment,
    shell: &mut Shell,
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

    // See if we need to eval an array index.
    // TODO: Handle associative arrays correctly; index is *not* an arithmetic expression
    if let Some(idx) = &array_index {
        let will_be_indexed_array = if let Some(existing_value) =
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
                    .eval(shell)
                    .await?
                    .to_string(),
            );
        }
    }

    // See if we can find an existing value associated with the variable.
    if let Some(existing_value) = shell.env.get_mut(variable_name.as_str()) {
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
    } else {
        // If not, we need to add it.
        let new_value = if let Some(array_index) = array_index {
            match new_value {
                ShellValueLiteral::Scalar(s) => ShellValue::indexed_array_from_literals(
                    ArrayLiteral(vec![(Some(array_index), s)]),
                )?,
                ShellValueLiteral::Array(_) => {
                    return error::unimp("cannot assign list to array member");
                }
            }
        } else {
            match new_value {
                ShellValueLiteral::Scalar(s) => ShellValue::String(s),
                ShellValueLiteral::Array(values) => {
                    ShellValue::indexed_array_from_literals(values)?
                }
            }
        };

        shell.env.add(
            variable_name,
            ShellVariable::new(new_value),
            EnvironmentScope::Global,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_lines)] // TODO: refactor this function
async fn execute_external_command(
    mut context: context::CommandExecutionContext<'_>,
    args: &[CommandArg],
    env_vars: &[(String, ShellValue)],
) -> Result<SpawnResult, error::Error> {
    let mut cmd = process::Command::new(context.command_name.clone());

    // Pass through args.
    for arg in args {
        if let CommandArg::String(s) = arg {
            cmd.arg(s);
        }
    }

    // Use the shell's current working dir.
    cmd.current_dir(context.shell.working_dir.as_path());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    for (name, var) in context.shell.env.iter() {
        if var.is_exported() {
            let value_as_str = var.value().to_cow_string();
            cmd.env(name, value_as_str.as_ref());
        }
    }

    // Overlay any variables explicitly provided as part of command execution.
    for (name, value) in env_vars {
        let value_as_str = value.to_cow_string();
        cmd.env(name, value_as_str.as_ref());
    }

    // Redirect stdin, if applicable.
    let mut stdin_here_doc = None;
    if let Some(stdin_file) = context.open_files.files.remove(&0) {
        if let OpenFile::HereDocument(doc) = &stdin_file {
            stdin_here_doc = Some(doc.clone());
        }

        let as_stdio: Stdio = stdin_file.into();
        cmd.stdin(as_stdio);
    }

    // Redirect stdout, if applicable.
    match context.open_files.files.remove(&1) {
        Some(OpenFile::Stdout) | None => (),
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.into();
            cmd.stdout(as_stdio);
        }
    }

    // Redirect stderr, if applicable.
    match context.open_files.files.remove(&2) {
        Some(OpenFile::Stderr) | None => {}
        Some(stderr_file) => {
            let as_stdio: Stdio = stderr_file.into();
            cmd.stderr(as_stdio);
        }
    }

    // Inject any other fds.
    let fd_mappings = context
        .open_files
        .files
        .iter()
        .map(|(child_fd, open_file)| FdMapping {
            child_fd: i32::try_from(*child_fd).unwrap(),
            parent_fd: open_file.as_raw_fd().unwrap(),
        })
        .collect();
    cmd.fd_mappings(fd_mappings)
        .map_err(|e| error::Error::Unknown(e.into()))?;

    log::debug!(
        "Spawning: {} {}",
        cmd.as_std().get_program().to_string_lossy().to_string(),
        cmd.as_std()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .join(" ")
    );

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
                log::error!(
                    "{}: {}: {}: not found",
                    context.shell.shell_name.as_ref().unwrap_or(&String::new()),
                    context.shell.get_current_input_line_number(),
                    context.command_name
                );
            } else {
                log::error!("{}: not found", context.command_name);
            }
            Ok(SpawnResult::ImmediateExit(127))
        }
        Err(e) => {
            log::error!("error: {}", e);
            Ok(SpawnResult::ImmediateExit(126))
        }
    }
}

async fn execute_builtin_command(
    builtin: builtin::BuiltinCommandExecuteFunc,
    context: context::CommandExecutionContext<'_>,
    args: Vec<CommandArg>,
    _env_vars: Vec<(String, ShellValue)>,
) -> Result<SpawnResult, error::Error> {
    let exit_code = match builtin(context, args).await {
        Ok(builtin_result) => match builtin_result.exit_code {
            builtin::BuiltinExitCode::Success => 0,
            builtin::BuiltinExitCode::InvalidUsage => 2,
            builtin::BuiltinExitCode::Unimplemented => 99,
            builtin::BuiltinExitCode::Custom(code) => code,
            builtin::BuiltinExitCode::ExitShell(code) => return Ok(SpawnResult::ExitShell(code)),
            builtin::BuiltinExitCode::ReturnFromFunctionOrScript(code) => {
                return Ok(SpawnResult::ReturnFromFunctionOrScript(code))
            }
            builtin::BuiltinExitCode::BreakLoop(count) => return Ok(SpawnResult::BreakLoop(count)),
            builtin::BuiltinExitCode::ContinueLoop(count) => {
                return Ok(SpawnResult::ContinueLoop(count))
            }
        },
        Err(e) => {
            log::error!("error: {}", e);
            1
        }
    };

    Ok(SpawnResult::ImmediateExit(exit_code))
}

pub(crate) async fn invoke_shell_function(
    mut context: context::CommandExecutionContext<'_>,
    args: &[CommandArg],
    env_vars: &[(String, ShellValue)],
) -> Result<SpawnResult, error::Error> {
    // TODO: We should figure out how to avoid cloning.
    let function_definition = context
        .shell
        .funcs
        .get(context.command_name.as_str())
        .unwrap()
        .clone();

    if !env_vars.is_empty() {
        log::error!("UNIMPLEMENTED: invoke function with environment variables");
    }

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

    // Note that we're going deeper.
    context
        .shell
        .enter_function(context.command_name.as_str(), &function_definition)?;

    // Pass through open files.
    let params = ExecutionParameters {
        open_files: context.open_files.clone(),
    };

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
) -> Result<()> {
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
        let (reader, writer) = os_pipe::pipe().map_err(|e| error::Error::Unknown(e.into()))?;
        context.output_pipes.push(reader);
        open_files.files.insert(1, OpenFile::PipeWriter(writer));
    }

    Ok(())
}

#[allow(clippy::too_many_lines)] // TODO: refactor this function
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
                .context(format!(
                    "opening {} for I/O redirection",
                    expanded_file_path.as_str()
                ))?;

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

                    let opened_file =
                        options.open(expanded_file_path.as_str()).context(format!(
                            "opening {} for I/O redirection",
                            expanded_file_path.as_str()
                        ))?;
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
                        log::error!("{}: Bad file descriptor", fd);
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
                            let (reader, writer) =
                                os_pipe::pipe().map_err(|e| error::Error::Unknown(e.into()))?;

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
