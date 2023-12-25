use std::collections::HashMap;
use std::io::Write;
use std::process::Stdio;

use anyhow::{Context, Result};

use itertools::Itertools;
use parser::ast::{self, CommandPrefixOrSuffixItem, IoFileRedirectTarget};

use crate::arithmetic::Evaluatable;
use crate::env::{EnvironmentLookup, EnvironmentScope};
use crate::expansion::WordExpander;
use crate::patterns;
use crate::shell::Shell;
use crate::variables::ShellValue;
use crate::{builtin, builtins};

#[derive(Default)]
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
    SpawnedChild(std::process::Child),
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

enum OpenFile {
    File(std::fs::File),
    HereDocument(String),
}

impl OpenFile {
    pub fn try_dup(&self) -> Result<OpenFile> {
        let result = match self {
            OpenFile::File(f) => OpenFile::File(f.try_clone()?),
            OpenFile::HereDocument(doc) => OpenFile::HereDocument(doc.clone()),
        };

        Ok(result)
    }
}

impl From<OpenFile> for Stdio {
    fn from(open_file: OpenFile) -> Self {
        match open_file {
            OpenFile::File(f) => f.into(),
            OpenFile::HereDocument(_) => Stdio::piped(),
        }
    }
}

struct OpenFiles {
    pub files: HashMap<u32, OpenFile>,
}

impl OpenFiles {
    pub fn new() -> OpenFiles {
        OpenFiles {
            files: HashMap::new(),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct ExecutionParameters {
    pub capture_output: bool,
}

pub trait Execute {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult>;
}

trait ExecuteInPipeline {
    fn execute_in_pipeline(&self, context: &mut PipelineExecutionContext) -> Result<SpawnResult>;
}

impl Execute for ast::Program {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult> {
        let mut result = ExecutionResult::success();

        for command in &self.complete_commands {
            result = command.execute(shell, params)?;
            if result.exit_shell || result.return_from_function_or_script {
                break;
            }
        }

        shell.last_exit_status = result.exit_code;
        Ok(result)
    }
}

impl Execute for ast::CompleteCommand {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult> {
        let mut result = ExecutionResult::success();

        for (ao_list, sep) in self {
            let run_async = matches!(sep, ast::SeparatorOperator::Async);

            if run_async {
                todo!("asynchronous execution")
            }

            result = ao_list.first.execute(shell, params)?;

            for next_ao in &ao_list.additional {
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

                result = pipeline.execute(shell, params)?;
            }
        }

        shell.last_exit_status = result.exit_code;
        Ok(result)
    }
}

impl Execute for ast::Pipeline {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult> {
        //
        // TODO: handle bang
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

        for command in self.seq.iter() {
            let spawn_result = command.execute_in_pipeline(&mut pipeline_context)?;
            pipeline_context.spawn_results.push(spawn_result);

            pipeline_context.current_pipeline_index += 1;
        }

        let mut result = ExecutionResult::success();

        let capture_output = pipeline_context.params.capture_output;
        let child_count = pipeline_context.spawn_results.len();
        for (child_index, child) in pipeline_context.spawn_results.into_iter().enumerate() {
            match child {
                SpawnResult::SpawnedChild(child) => {
                    let output = child.wait_with_output()?;
                    let exit_code: u8 = (output.status.code().unwrap_or(127) & 0xFF) as u8;

                    // TODO: Confirm what to return if it was signaled.
                    result = ExecutionResult::new(exit_code);

                    if capture_output && child_index + 1 == child_count {
                        let output_str = std::str::from_utf8(output.stdout.as_slice())?;
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

        shell.last_exit_status = result.exit_code;
        Ok(result)
    }
}

impl ExecuteInPipeline for ast::Command {
    fn execute_in_pipeline(
        &self,
        pipeline_context: &mut PipelineExecutionContext,
    ) -> Result<SpawnResult> {
        match self {
            ast::Command::Simple(simple) => simple.execute_in_pipeline(pipeline_context),
            ast::Command::Compound(compound, _redirects) => {
                //
                // TODO: handle redirects
                // TODO: Need to execute in the pipeline.
                //

                let result = compound.execute(pipeline_context.shell, &pipeline_context.params)?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
            // TODO: Need to execute in pipeline.
            ast::Command::Function(func) => {
                let result = func.execute(pipeline_context.shell, &pipeline_context.params)?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
            ast::Command::ExtendedTest(e) => Ok(SpawnResult::ImmediateExit(eval_expression(
                e,
                pipeline_context.shell,
            )?)),
        }
    }
}

fn eval_expression(expr: &ast::ExtendedTestExpression, shell: &mut Shell) -> Result<u8> {
    let result = match expr {
        ast::ExtendedTestExpression::StringsAreEqual(left, right) => {
            let expanded_str = expand_word(shell, left)?;
            let expanded_pattern = expand_word(shell, right)?;
            let matches =
                patterns::pattern_matches(expanded_pattern.as_str(), expanded_str.as_str())?;
            if matches {
                0
            } else {
                1
            }
        }
        _ => {
            // TODO: implement eval_expression
            log::error!("UNIMPLEMENTED: eval test expression: {:?}", expr);
            0
        }
    };

    Ok(result)
}

enum WhileOrUtil {
    While,
    Util,
}

impl Execute for ast::CompoundCommand {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult> {
        match self {
            ast::CompoundCommand::BraceGroup(g) => g.execute(shell, params),
            ast::CompoundCommand::Subshell(s) => {
                // TODO: actually implement subshell semantics
                // TODO: for that matter, look at shell properties in builtin invocation
                s.execute(shell, params)
            }
            ast::CompoundCommand::ForClause(f) => f.execute(shell, params),
            ast::CompoundCommand::CaseClause(c) => c.execute(shell, params),
            ast::CompoundCommand::IfClause(i) => i.execute(shell, params),
            ast::CompoundCommand::WhileClause(w) => (WhileOrUtil::While, w).execute(shell, params),
            ast::CompoundCommand::UntilClause(u) => (WhileOrUtil::Util, u).execute(shell, params),
            ast::CompoundCommand::Arithmetic(a) => a.execute(shell, params),
        }
    }
}

impl Execute for ast::ForClauseCommand {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult> {
        let mut result = ExecutionResult::success();

        if let Some(unexpanded_values) = &self.values {
            let expanded_values = unexpanded_values
                .iter()
                .map(|v| expand_word(shell, v))
                .collect::<Result<Vec<_>>>()?;

            for value in expanded_values {
                // Update the variable.
                shell.env.update_or_add(
                    &self.variable_name,
                    value.as_str(),
                    |_| Ok(()),
                    EnvironmentLookup::Anywhere,
                    EnvironmentScope::Global,
                )?;

                result = self.body.execute(shell, params)?;
            }
        }

        shell.last_exit_status = result.exit_code;
        Ok(result)
    }
}

impl Execute for ast::CaseClauseCommand {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult> {
        let expanded_value = expand_word(shell, &self.value)?;
        for case in self.cases.iter() {
            let mut matches = false;

            for pattern in case.patterns.iter() {
                let expanded_pattern = expand_word(shell, pattern)?;
                if patterns::pattern_matches(expanded_pattern.as_str(), expanded_value.as_str())? {
                    matches = true;
                    break;
                }
            }

            if matches {
                if let Some(case_cmd) = &case.cmd {
                    return case_cmd.execute(shell, params);
                }
            }
        }

        let result = ExecutionResult::success();
        shell.last_exit_status = result.exit_code;

        Ok(result)
    }
}

impl Execute for ast::IfClauseCommand {
    fn execute(&self, shell: &mut Shell, params: &ExecutionParameters) -> Result<ExecutionResult> {
        let condition = self.condition.execute(shell, params)?;

        if condition.is_success() {
            return self.then.execute(shell, params);
        }

        if let Some(elses) = &self.elses {
            for else_clause in elses {
                match &else_clause.condition {
                    Some(else_condition) => {
                        let else_condition_result = else_condition.execute(shell, params)?;
                        if else_condition_result.is_success() {
                            return else_clause.body.execute(shell, params);
                        }
                    }
                    None => {
                        return else_clause.body.execute(shell, params);
                    }
                }
            }
        }

        let result = ExecutionResult::success();
        shell.last_exit_status = result.exit_code;

        Ok(result)
    }
}

impl Execute for (WhileOrUtil, &ast::WhileClauseCommand) {
    fn execute(
        &self,
        _shell: &mut Shell,
        _params: &ExecutionParameters,
    ) -> Result<ExecutionResult> {
        todo!("execute while clause command")
    }
}

impl Execute for ast::ArithmeticCommand {
    fn execute(&self, shell: &mut Shell, _params: &ExecutionParameters) -> Result<ExecutionResult> {
        let value = self.expr.eval(shell)?;
        let result = if value == 0 {
            ExecutionResult::success()
        } else {
            ExecutionResult::new(1)
        };

        shell.last_exit_status = result.exit_code;

        Ok(result)
    }
}

impl Execute for ast::FunctionDefinition {
    fn execute(&self, shell: &mut Shell, _params: &ExecutionParameters) -> Result<ExecutionResult> {
        //
        // TODO: confirm whether defining a function resets the last execution.
        //

        shell.funcs.insert(self.fname.clone(), self.clone());

        let result = ExecutionResult::success();
        shell.last_exit_status = result.exit_code;

        Ok(result)
    }
}

impl ExecuteInPipeline for ast::SimpleCommand {
    fn execute_in_pipeline(&self, context: &mut PipelineExecutionContext) -> Result<SpawnResult> {
        let mut redirects = vec![];
        let mut env_vars = vec![];

        if let Some(prefix_items) = &self.prefix {
            for item in prefix_items {
                match item {
                    CommandPrefixOrSuffixItem::IoRedirect(r) => redirects.push(r),
                    CommandPrefixOrSuffixItem::AssignmentWord(pair) => env_vars.push(pair),
                    CommandPrefixOrSuffixItem::Word(_w) => {
                        // This should not happen.
                    }
                }
            }
        }

        //
        // 2. The words that are not variable assignments or redirections shall be expanded.
        // If any fields remain following their expansion, the first field shall be considered
        // the command name and remaining fields are the arguments for the command.
        //

        let mut args = vec![];

        if let Some(cmd_name) = &self.word_or_name {
            args.push(cmd_name);
        }

        if let Some(suffix_items) = &self.suffix {
            for item in suffix_items {
                match item {
                    CommandPrefixOrSuffixItem::IoRedirect(r) => redirects.push(r),
                    CommandPrefixOrSuffixItem::Word(arg) => args.push(arg),
                    CommandPrefixOrSuffixItem::AssignmentWord(_) => {
                        // This should not happen.
                    }
                }
            }
        }

        // Expand the command words.
        // TODO: Deal with the fact that an expansion might introduce multiple fields.
        let mut args: Vec<String> = args
            .iter()
            .map(|a| expand_word(context.shell, a))
            .collect::<Result<Vec<_>>>()?;

        //
        // 3. Redirections shall be performed.
        //

        let mut open_files = OpenFiles::new();
        if !redirects.is_empty() {
            for redirect in redirects.into_iter() {
                match redirect {
                    ast::IoRedirect::File(fd_num, kind, target) => {
                        // If not specified, we default fd to stdout.
                        // TODO: Validate this is correct.
                        let fd_num = fd_num.unwrap_or(1);

                        let target_file;
                        match target {
                            IoFileRedirectTarget::Filename(f) => {
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

                                let expanded_file_path = expand_word(context.shell, f)?;

                                let opened_file =
                                    options.open(expanded_file_path.as_str()).context(format!(
                                        "opening {} for I/O redirection",
                                        expanded_file_path.as_str()
                                    ))?;
                                target_file = OpenFile::File(opened_file);
                            }
                            IoFileRedirectTarget::Fd(fd) => {
                                if let Some(f) = open_files.files.get(fd) {
                                    target_file = f.try_dup()?;
                                } else {
                                    log::error!("{}: Bad file descriptor", fd);
                                    return Ok(SpawnResult::ImmediateExit(1));
                                }
                            }
                            IoFileRedirectTarget::ProcessSubstitution(subshell_cmd) => {
                                log::error!(
                                    "UNIMPLEMENTED: process substitution with command: {:?}",
                                    subshell_cmd
                                );
                                todo!("process substitution")
                            }
                        }

                        open_files.files.insert(fd_num, target_file);
                    }
                    ast::IoRedirect::HereDocument(fd_num, io_here) => {
                        // If not specified, default to stdin (fd 0).
                        let fd_num = fd_num.unwrap_or(0);

                        // TODO: figure out if we need to expand?
                        let io_here_doc = io_here.doc.flatten();

                        open_files
                            .files
                            .insert(fd_num, OpenFile::HereDocument(io_here_doc));
                    }
                    ast::IoRedirect::HereString(fd_num, word) => {
                        // If not specified, default to stdin (fd 0).
                        let fd_num = fd_num.unwrap_or(0);

                        let mut expanded_word = expand_word(context.shell, word)?;
                        expanded_word.push('\n');

                        open_files
                            .files
                            .insert(fd_num, OpenFile::HereDocument(expanded_word));
                    }
                }
            }
        }

        //
        // 4. Each variable assignment shall be expanded for tilde expansion, parameter
        // expansion, command substitution, arithmetic expansion, and quote removal
        // prior to assigning the value.
        //

        let env_vars: Vec<(String, ShellValue)> = env_vars
            .iter()
            .map(|assignment| match assignment {
                ast::Assignment::Scalar { name, value } => {
                    let expanded_value = expand_word(context.shell, value)?;
                    Ok((name.clone(), ShellValue::String(expanded_value)))
                }
                ast::Assignment::Array { name, values } => {
                    let expanded_values = values
                        .iter()
                        .map(|v| expand_word(context.shell, v))
                        .collect::<Result<Vec<_>>>()?;
                    Ok((name.clone(), ShellValue::IndexedArray(expanded_values)))
                }
            })
            .collect::<Result<Vec<_>>>()?;

        if let Some(cmd_name) = &self.word_or_name {
            let mut cmd_name = expand_word(context.shell, cmd_name)?;

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
                if let Some(builtin) = builtins::SPECIAL_BUILTINS.get(cmd_name.as_str()) {
                    execute_builtin_command(builtin, context, &args, &env_vars)
                } else if context.shell.funcs.contains_key(&cmd_name) {
                    // Strip the function name off args.
                    invoke_shell_function(context, cmd_name.as_str(), &args[1..], &env_vars)
                } else if let Some(builtin) = builtins::BUILTINS.get(cmd_name.as_str()) {
                    execute_builtin_command(builtin, context, &args, &env_vars)
                } else {
                    // Strip the command name off args.
                    execute_external_command(
                        context,
                        &mut open_files,
                        cmd_name.as_ref(),
                        &args[1..],
                        &env_vars,
                    )
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
            }
        } else {
            //
            // This must just be an assignment.
            //

            for (name, value) in env_vars {
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

fn execute_external_command(
    context: &mut PipelineExecutionContext,
    open_files: &mut OpenFiles,
    cmd_name: &str,
    args: &[String],
    env_vars: &[(String, ShellValue)],
) -> Result<SpawnResult> {
    let mut cmd = std::process::Command::new(cmd_name);

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
    if let Some(stdout_file) = open_files.files.remove(&1) {
        let as_stdio: Stdio = stdout_file.into();
        cmd.stdout(as_stdio);
        redirected_stdout = true;
    } else {
        redirected_stdout = false;
    }

    // If we were asked to capture the output of this command (and if it's the last command
    // in the pipeline), then we need to arrange to redirect output to a pipe that we can
    // read later.
    if context.params.capture_output && context.pipeline_len == context.current_pipeline_index + 1 {
        if redirected_stdout {
            log::warn!(
                "UNIMPLEMENTED: {}: output redirection used in command substitution",
                context.shell.shell_name.as_ref().map_or("", |sn| sn),
            );
        } else {
            cmd.stdout(std::process::Stdio::piped());
            redirected_stdout = true;
        }
    }

    // Redirect stderr, if applicable.
    if let Some(stderr_file) = open_files.files.remove(&2) {
        let as_stdio: Stdio = stderr_file.into();
        cmd.stderr(as_stdio);
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
                        // Set up stdin of this process to take stdout of the preceding process.
                        cmd.stdin(std::process::Stdio::from(child.stdout.take().unwrap()));
                    }
                    SpawnResult::ImmediateExit(_code)
                    | SpawnResult::ExitShell(_code)
                    | SpawnResult::ReturnFromFunctionOrScript(_code) => {
                        return Err(anyhow::anyhow!("unable to retrieve piped command output"));
                    }
                }

                // Push it back so we can wait on it later.
                context.spawn_results.push(preceding_result);
            }
        }

        if context.current_pipeline_index < context.pipeline_len - 1 {
            // Set up stdout of this process to go to stdin of the succeeding process.
            cmd.stdout(std::process::Stdio::piped());
        }
    }

    log::debug!(
        "Spawning: {} {}",
        cmd.get_program().to_string_lossy().to_string(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .join(" ")
    );

    match cmd.spawn() {
        Ok(mut child) => {
            log::debug!("Process spawned: {}", child.id());

            // Special case: handle writing here document, if needed.
            if let Some(doc) = stdin_here_doc {
                if let Some(mut child_stdin) = child.stdin.take() {
                    child_stdin.write_all(doc.as_bytes())?;
                }
            }

            Ok(SpawnResult::SpawnedChild(child))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::error!("command not found: {}", cmd_name);
            Ok(SpawnResult::ImmediateExit(127))
        }
        Err(e) => Err(e.into()),
    }
}

fn execute_builtin_command(
    builtin: &builtin::BuiltinCommandExecuteFunc,
    context: &mut PipelineExecutionContext,
    args: &[String],
    _env_vars: &[(String, ShellValue)],
) -> Result<SpawnResult> {
    let args: Vec<_> = args.iter().map(AsRef::as_ref).collect();
    let mut builtin_context = builtin::BuiltinExecutionContext {
        shell: context.shell,
        builtin_name: args[0],
    };
    let builtin_result = builtin(&mut builtin_context, args.as_slice())?;

    let exit_code = match builtin_result.exit_code {
        builtin::BuiltinExitCode::Success => 0,
        builtin::BuiltinExitCode::InvalidUsage => 2,
        builtin::BuiltinExitCode::Unimplemented => 99,
        builtin::BuiltinExitCode::Custom(code) => code,
        builtin::BuiltinExitCode::ExitShell(code) => return Ok(SpawnResult::ExitShell(code)),
        builtin::BuiltinExitCode::ReturnFromFunctionOrScript(code) => {
            return Ok(SpawnResult::ReturnFromFunctionOrScript(code))
        }
    };

    Ok(SpawnResult::ImmediateExit(exit_code))
}

fn invoke_shell_function(
    context: &mut PipelineExecutionContext,
    cmd_name: &str,
    args: &[String],
    env_vars: &[(String, ShellValue)],
) -> Result<SpawnResult> {
    // TODO: We should figure out how to avoid cloning.
    let function_definition = context.shell.funcs.get(cmd_name).unwrap().clone();

    if !env_vars.is_empty() {
        log::error!("UNIMPLEMENTED: invoke function with environment variables");
    }

    let (body, redirects) = &function_definition.body;
    if redirects.is_some() {
        log::error!("UNIMPLEMENTED: invoke function with redirects");
    }

    // Temporarily replace positional parameters.
    let prior_positional_params = std::mem::take(&mut context.shell.positional_parameters);
    context.shell.positional_parameters = args.to_owned();

    // Note that we're going deeper.
    context.shell.enter_function();

    // Invoke the function.
    let result = body.execute(context.shell, &ExecutionParameters::default());

    // We've come back out, reflect it.
    context.shell.leave_function();

    // Restore positional parameters.
    context.shell.positional_parameters = prior_positional_params;

    Ok(SpawnResult::ImmediateExit(result?.exit_code))
}

fn expand_word(shell: &mut Shell, word: &ast::Word) -> Result<String> {
    let mut expander = WordExpander::new(shell);
    expander.expand(word.flatten().as_str())
}
