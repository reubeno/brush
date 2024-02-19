use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::process::ExitStatusExt;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process;

use anyhow::{Context, Result};
use itertools::Itertools;
use parser::ast::{self, CommandPrefixOrSuffixItem};
use tokio_command_fds::{CommandFdExt, FdMapping};

use crate::arithmetic::Evaluatable;
use crate::env::{EnvironmentLookup, EnvironmentScope};
use crate::error;
use crate::expansion::expand_word;
use crate::openfiles::{OpenFile, OpenFiles};
use crate::shell::Shell;
use crate::variables::{self, ShellValue};
use crate::{builtin, builtins};
use crate::{extendedtests, patterns};

#[derive(Debug, Default)]
pub struct ExecutionResult {
    pub exit_code: u8,
    pub exit_shell: bool,
    pub return_from_function_or_script: bool,
    pub output: Option<String>,
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

enum SpawnResult {
    SpawnedChild(process::Child),
    ImmediateExit(u8),
    ExitShell(u8),
    ReturnFromFunctionOrScript(u8),
}

struct PipelineExecutionContext<'a> {
    shell: &'a mut Shell,

    current_pipeline_index: usize,
    pipeline_len: usize,

    spawn_results: Vec<SpawnResult>,

    params: ExecutionParameters,
}

#[derive(Clone, Default, Debug)]
pub struct ExecutionParameters {
    pub capture_output: bool,
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

                // TODO: don't always log to stdout! Shouldn't if we're in a script or non-interactive?
                println!("[{job_number}] <pid unknown>");
            } else {
                result = ao_list.execute(shell, params).await?;
            }

            // Check for early return.
            if result.return_from_function_or_script {
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
        // TODO: confirm whether exit code comes from first or last in pipeline
        //

        let mut pipeline_context = PipelineExecutionContext {
            shell,
            current_pipeline_index: 0,
            pipeline_len: self.seq.len(),
            spawn_results: vec![],
            params: params.clone(),
        };

        for command in &self.seq {
            let spawn_result = command.execute_in_pipeline(&mut pipeline_context).await?;
            pipeline_context.spawn_results.push(spawn_result);

            pipeline_context.current_pipeline_index += 1;
        }

        let mut result = ExecutionResult::success();

        let capture_output = pipeline_context.params.capture_output;
        let child_count = pipeline_context.spawn_results.len();
        for (child_index, child) in pipeline_context.spawn_results.into_iter().enumerate() {
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

                    if capture_output && child_index + 1 == child_count {
                        let output_str = std::str::from_utf8(output.stdout.as_slice())
                            .map_err(|e| error::Error::Unknown(e.into()))?;
                        result.output = Some(output_str.to_owned());
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
            ast::Command::Compound(compound, _redirects) => {
                //
                // TODO: handle redirects
                // TODO: Need to execute in the pipeline.
                //

                let result = compound
                    .execute(pipeline_context.shell, &pipeline_context.params)
                    .await?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
            // TODO: Need to execute in pipeline.
            ast::Command::Function(func) => {
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
                // TODO: actually implement subshell semantics
                // TODO: for that matter, look at shell properties in builtin invocation
                s.execute(shell, params).await
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
            let mut expanded_values = vec![];
            for value in unexpanded_values {
                expanded_values.push(expand_word(shell, value).await?);
            }

            for value in expanded_values {
                // Update the variable.
                shell.env.update_or_add(
                    &self.variable_name,
                    value.as_str(),
                    |_| Ok(()),
                    EnvironmentLookup::Anywhere,
                    EnvironmentScope::Global,
                )?;

                result = self.body.0.execute(shell, params).await?;
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
        let expanded_value = expand_word(shell, &self.value).await?;
        for case in &self.cases {
            let mut matches = false;

            for pattern in &case.patterns {
                let expanded_pattern = expand_word(shell, pattern).await?;
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
        //
        // TODO: confirm whether defining a function resets the last execution.
        //

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
        let empty_prefix = ast::CommandPrefix(vec![]);
        let prefix_items = self.prefix.as_ref().unwrap_or(&empty_prefix);

        let empty_suffix = ast::CommandSuffix(vec![]);
        let suffix_items = self.suffix.as_ref().unwrap_or(&empty_suffix);
        let mut cmd_name_items = vec![];
        if let Some(cmd_name) = &self.word_or_name {
            cmd_name_items.push(CommandPrefixOrSuffixItem::Word(cmd_name.clone()));
        }

        let mut open_files = OpenFiles::new();
        let mut assignments = vec![];
        let mut args = vec![];

        for item in prefix_items
            .0
            .iter()
            .chain(cmd_name_items.iter())
            .chain(suffix_items.0.iter())
        {
            match item {
                CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                    if let Some(open_file) =
                        setup_redirect(&mut open_files, context.shell, redirect).await?
                    {
                        // TODO: Don't just count on the fd being valid.
                        if let OpenFile::ProcessSubstitutionFile(f) = open_file {
                            args.push(std::format!("/dev/fd/{}", f.as_raw_fd()));
                        }
                    } else {
                        return Ok(SpawnResult::ImmediateExit(1));
                    }
                }
                CommandPrefixOrSuffixItem::AssignmentWord(pair) => assignments.push(pair),
                CommandPrefixOrSuffixItem::Word(arg) => {
                    // TODO: Deal with the fact that an expansion might introduce multiple fields.
                    let expanded_arg = expand_word(context.shell, arg).await?;
                    args.push(expanded_arg);
                }
            }
        }

        let mut env_vars = vec![];

        for assignment in &assignments {
            match assignment {
                ast::Assignment::Scalar {
                    name,
                    value,
                    append,
                } => {
                    let mut value = value.clone();

                    if *append {
                        if let Some(prev_value) =
                            context.shell.env.get(name).map(|v| v.value.clone())
                        {
                            // TODO: Find a cleaner way to do this.
                            let mut prev_str: String = (&prev_value).into();
                            prev_str.push_str(value.value.as_str());
                            value = ast::Word { value: prev_str }
                        }
                    }

                    let expanded_value = expand_word(context.shell, &value).await?;
                    env_vars.push((name.clone(), ShellValue::String(expanded_value)));
                }
                ast::Assignment::Array {
                    name,
                    values,
                    append,
                } => {
                    if *append {
                        log::error!("UNIMPLEMENTED: append assignment");
                    }

                    let mut expanded_values = vec![];
                    for value in values {
                        expanded_values.push(expand_word(context.shell, value).await?);
                    }
                    env_vars.push((name.clone(), ShellValue::IndexedArray(expanded_values)));
                }
            }
        }

        if let Some(cmd_name) = &self.word_or_name {
            let mut cmd_name = expand_word(context.shell, cmd_name).await?;

            if context.shell.options.print_commands_and_arguments {
                println!("+ {}", args.join(" "));
            }

            //
            // TODO: Reevaluate if this is an appropriate place to handle aliases.
            //
            if let Some(alias_value) = context.shell.aliases.get(&cmd_name) {
                //
                // TODO: This is a total hack.
                //
                for (i, alias_piece) in alias_value.split_ascii_whitespace().enumerate() {
                    if i == 0 {
                        cmd_name = alias_piece.to_owned();
                        args[0] = alias_piece.to_owned();
                    } else {
                        args.insert(i, alias_piece.to_owned());
                    }
                }
            }

            if !cmd_name.contains('/') {
                // TODO: Cache these.
                let special_builtins = builtins::get_special_builtins();
                let builtins = builtins::get_builtins();

                // TODO: cache the builtins
                if let Some(builtin) = special_builtins.get(cmd_name.as_str()) {
                    execute_builtin_command(*builtin, context, args, env_vars).await
                } else if context.shell.funcs.contains_key(&cmd_name) {
                    // Strip the function name off args.
                    invoke_shell_function(context, cmd_name.as_str(), &args[1..], &env_vars).await
                } else if let Some(builtin) = builtins.get(cmd_name.as_str()) {
                    execute_builtin_command(*builtin, context, args, env_vars).await
                } else {
                    // Strip the command name off args.
                    execute_external_command(
                        context,
                        &mut open_files,
                        cmd_name.as_ref(),
                        &args[1..],
                        &env_vars,
                    )
                    .await
                }
            } else {
                // Strip the command name off args.
                execute_external_command(
                    context,
                    &mut open_files,
                    cmd_name.as_ref(),
                    &args[1..],
                    &env_vars,
                )
                .await
            }
        } else {
            //
            // This must just be an assignment.
            //

            for (name, value) in env_vars {
                if context.shell.options.print_commands_and_arguments {
                    println!("+ {name}={}", value.format(variables::FormatStyle::Basic)?);
                }

                // TODO: Handle readonly variables.
                context.shell.env.update_or_add(
                    name,
                    value,
                    |_| Ok(()),
                    EnvironmentLookup::Anywhere,
                    EnvironmentScope::Global,
                )?;
            }

            Ok(SpawnResult::ImmediateExit(0))
        }
    }
}

#[allow(clippy::too_many_lines)] // TODO: refactor this function
async fn execute_external_command(
    context: &mut PipelineExecutionContext<'_>,
    open_files: &mut OpenFiles,
    cmd_name: &str,
    args: &[String],
    env_vars: &[(String, ShellValue)],
) -> Result<SpawnResult, error::Error> {
    let mut cmd = process::Command::new(cmd_name);

    // Pass through args.
    for arg in args {
        cmd.arg(arg);
    }

    // Use the shell's current working dir.
    cmd.current_dir(context.shell.working_dir.as_path());

    // Start with a clear environment.
    cmd.env_clear();

    // Add in exported variables.
    for (name, var) in context.shell.env.iter() {
        if var.exported {
            cmd.env(name, &String::from(&var.value));
        }
    }

    // Overlay any variables explicitly provided as part of command execution.
    for (name, value) in env_vars {
        let value_as_str: String = value.into();
        cmd.env(name, value_as_str);
    }

    // Redirect stdin, if applicable.
    let mut stdin_here_doc = None;
    if let Some(stdin_file) = open_files.files.remove(&0) {
        if let OpenFile::HereDocument(doc) = &stdin_file {
            stdin_here_doc = Some(doc.clone());
        }

        let as_stdio: Stdio = stdin_file.into();
        cmd.stdin(as_stdio);
    }

    // Redirect stdout, if applicable.
    let mut redirected_stdout;
    match open_files.files.remove(&1) {
        Some(OpenFile::Stdout) | None => {
            redirected_stdout = false;
        }
        Some(stdout_file) => {
            let as_stdio: Stdio = stdout_file.into();
            cmd.stdout(as_stdio);
            redirected_stdout = true;
        }
    }

    // If we were asked to capture the output of this command (and if it's the last command
    // in the pipeline), then we need to arrange to redirect output to a pipe that we can
    // read later.
    if context.params.capture_output && context.pipeline_len == context.current_pipeline_index + 1 {
        if redirected_stdout {
            log::warn!(
                "UNIMPLEMENTED: {}: output redirection used in command substitution; command=[{} {}]",
                context.shell.shell_name.as_ref().map_or("", |sn| sn),
                cmd.as_std().get_program().to_string_lossy(),
                cmd.as_std().get_args().map(|a| a.to_string_lossy().to_string()).join(" "),
            );
        } else {
            cmd.stdout(Stdio::piped());
            redirected_stdout = true;
        }
    }

    // Redirect stderr, if applicable.
    match open_files.files.remove(&2) {
        Some(OpenFile::Stderr) | None => {}
        Some(stderr_file) => {
            let as_stdio: Stdio = stderr_file.into();
            cmd.stderr(as_stdio);
        }
    }

    // Inject any appropriate fds.
    let open_files_keys = open_files.files.keys().copied().collect::<Vec<_>>();
    for key in open_files_keys {
        if let Some(OpenFile::ProcessSubstitutionFile(temp_file)) = open_files.files.remove(&key) {
            #[allow(clippy::cast_possible_wrap)]
            cmd.fd_mappings(vec![FdMapping {
                child_fd: key as i32,
                parent_fd: temp_file.as_raw_fd(),
            }])
            .map_err(|e| error::Error::Unknown(e.into()))?;
        }
    }

    // See if we need to set up piping.
    if context.pipeline_len > 1 {
        // TODO: Handle stderr/other redirects/etc.
        if (context.current_pipeline_index < context.pipeline_len - 1) && redirected_stdout {
            log::warn!(
                "UNIMPLEMENTED: {}: mix of redirection and pipes in command '{}'",
                context.shell.shell_name.as_ref().map_or("", |sn| sn),
                cmd_name,
            );
        }

        if context.current_pipeline_index > 0 {
            // Find the stdout from the preceding process.
            if let Some(mut preceding_result) = context.spawn_results.pop() {
                match &mut preceding_result {
                    SpawnResult::SpawnedChild(child) => {
                        let stream_fd = child.stdout.take().unwrap().into_owned_fd()?;
                        let stream_file: std::fs::File = stream_fd.into();

                        // Set up stdin of this process to take stdout of the preceding process.
                        cmd.stdin(stream_file);
                    }
                    SpawnResult::ImmediateExit(_code)
                    | SpawnResult::ExitShell(_code)
                    | SpawnResult::ReturnFromFunctionOrScript(_code) => {
                        log::error!("UNIMPLEMENTED: unable to retrieve piped command output");
                        cmd.stdin(Stdio::null());
                    }
                }

                // Push it back so we can wait on it later.
                context.spawn_results.push(preceding_result);
            }
        }

        if context.current_pipeline_index < context.pipeline_len - 1 {
            // Set up stdout of this process to go to stdin of the succeeding process.
            cmd.stdout(Stdio::piped());
        }
    }

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
            log::error!("command not found: {}", cmd_name);
            Ok(SpawnResult::ImmediateExit(127))
        }
        Err(e) => {
            log::error!("error: {}", e);
            Ok(SpawnResult::ImmediateExit(126))
        }
    }
}

async fn execute_builtin_command<'a>(
    builtin: builtin::BuiltinCommandExecuteFunc,
    context: &'a mut PipelineExecutionContext<'_>,
    args: Vec<String>,
    _env_vars: Vec<(String, ShellValue)>,
) -> Result<SpawnResult, error::Error> {
    let builtin_context = builtin::BuiltinExecutionContext {
        shell: context.shell,
        builtin_name: args[0].clone(),
    };

    let exit_code = match builtin(builtin_context, args).await {
        Ok(builtin_result) => match builtin_result.exit_code {
            builtin::BuiltinExitCode::Success => 0,
            builtin::BuiltinExitCode::InvalidUsage => 2,
            builtin::BuiltinExitCode::Unimplemented => 99,
            builtin::BuiltinExitCode::Custom(code) => code,
            builtin::BuiltinExitCode::ExitShell(code) => return Ok(SpawnResult::ExitShell(code)),
            builtin::BuiltinExitCode::ReturnFromFunctionOrScript(code) => {
                return Ok(SpawnResult::ReturnFromFunctionOrScript(code))
            }
        },
        Err(e) => {
            log::error!("error: {}", e);
            1
        }
    };

    Ok(SpawnResult::ImmediateExit(exit_code))
}

#[async_recursion::async_recursion]
async fn invoke_shell_function(
    context: &mut PipelineExecutionContext<'_>,
    cmd_name: &str,
    args: &[String],
    env_vars: &[(String, ShellValue)],
) -> Result<SpawnResult, error::Error> {
    // TODO: We should figure out how to avoid cloning.
    let function_definition = context.shell.funcs.get(cmd_name).unwrap().clone();

    if !env_vars.is_empty() {
        log::error!("UNIMPLEMENTED: invoke function with environment variables");
    }

    let ast::FunctionBody(body, redirects) = &function_definition.body;
    if redirects.is_some() {
        log::error!("UNIMPLEMENTED: invoke function with redirects");
    }

    // Temporarily replace positional parameters.
    let prior_positional_params = std::mem::take(&mut context.shell.positional_parameters);
    context.shell.positional_parameters = args.to_owned();

    // Note that we're going deeper.
    context.shell.enter_function();

    // Invoke the function.
    let result = body
        .execute(context.shell, &ExecutionParameters::default())
        .await;

    // We've come back out, reflect it.
    context.shell.leave_function();

    // Restore positional parameters.
    context.shell.positional_parameters = prior_positional_params;

    Ok(SpawnResult::ImmediateExit(result?.exit_code))
}

#[allow(clippy::too_many_lines)] // TODO: refactor this function
#[async_recursion::async_recursion]
async fn setup_redirect<'a>(
    open_files: &'a mut OpenFiles,
    shell: &mut Shell,
    redirect: &ast::IoRedirect,
) -> Result<Option<&'a OpenFile>> {
    match redirect {
        ast::IoRedirect::File(fd_num, kind, target) => {
            // If not specified, we default fd to stdout.
            // TODO: Validate this is correct.
            let mut fd_num = fd_num.unwrap_or(1);

            let target_file;
            match target {
                ast::IoFileRedirectTarget::Filename(f) => {
                    let mut options = std::fs::File::options();

                    match kind {
                        ast::IoFileRedirectKind::Read => {
                            options.read(true);
                        }
                        ast::IoFileRedirectKind::Write => {
                            // TODO: observe noclobber options
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

                    let expanded_file_path = expand_word(shell, f).await?;

                    let opened_file =
                        options.open(expanded_file_path.as_str()).context(format!(
                            "opening {} for I/O redirection",
                            expanded_file_path.as_str()
                        ))?;
                    target_file = OpenFile::File(opened_file);
                }
                ast::IoFileRedirectTarget::Fd(fd) => {
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
                        #[allow(clippy::cast_sign_loss)]
                        ast::IoFileRedirectKind::Read => {
                            // TODO: Don't execute synchronously!
                            let text_results = subshell_cmd
                                .execute(
                                    shell,
                                    &ExecutionParameters {
                                        capture_output: true,
                                    },
                                )
                                .await?
                                .output
                                .unwrap();

                            let mut temp_file = tempfile::tempfile()?;
                            temp_file.write_all(text_results.as_bytes())?;

                            // TODO: Don't just count on the fd being valid.
                            fd_num = temp_file.as_raw_fd() as u32;

                            target_file = OpenFile::ProcessSubstitutionFile(temp_file);
                        }
                        ast::IoFileRedirectKind::Write => {
                            log::error!(
                                "UNIMPLEMENTED: process substitution to write to stdin of command: {:?}",
                                subshell_cmd
                            );
                            return Err(anyhow::anyhow!(
                                "UNIMPLEMENTED: process substitution to write to command"
                            ));
                        }
                        _ => return Err(anyhow::anyhow!("invalid process substitution")),
                    }
                }
            }

            open_files.files.insert(fd_num, target_file);
            return Ok(Some(open_files.files.get(&fd_num).unwrap()));
        }
        ast::IoRedirect::HereDocument(fd_num, io_here) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            // TODO: figure out if we need to expand?
            let io_here_doc = io_here.doc.flatten();

            open_files
                .files
                .insert(fd_num, OpenFile::HereDocument(io_here_doc));
            return Ok(Some(open_files.files.get(&fd_num).unwrap()));
        }
        ast::IoRedirect::HereString(fd_num, word) => {
            // If not specified, default to stdin (fd 0).
            let fd_num = fd_num.unwrap_or(0);

            let mut expanded_word = expand_word(shell, word).await?;
            expanded_word.push('\n');

            open_files
                .files
                .insert(fd_num, OpenFile::HereDocument(expanded_word));
            return Ok(Some(open_files.files.get(&fd_num).unwrap()));
        }
    }
}
