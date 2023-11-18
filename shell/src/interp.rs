use anyhow::Result;

use log::error;
use parser::ast::{self, CommandPrefixOrSuffixItem};

use crate::context::ExecutionContext;
use crate::expansion::WordExpander;
use crate::{builtin, builtins};

pub struct ExecutionResult {
    pub exit_code: i32,
}

impl ExecutionResult {
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

enum SpawnResult {
    SpawnedChild(std::process::Child),
    ImmediateExit(i32),
}

struct PipelineExecutionContext<'a> {
    context: &'a mut ExecutionContext,

    current_pipeline_index: usize,
    pipeline_len: usize,

    spawn_results: Vec<SpawnResult>,
}

pub trait Execute {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult>;
}

trait ExecuteInPipeline {
    fn execute_in_pipeline(&self, context: &mut PipelineExecutionContext) -> Result<SpawnResult>;
}

impl Execute for ast::Program {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult> {
        let mut result = ExecutionResult { exit_code: 0 };

        for command in &self.complete_commands {
            result = command.execute(context)?;
        }

        Ok(result)
    }
}

impl Execute for ast::CompleteCommand {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult> {
        let mut result = ExecutionResult { exit_code: 0 };

        for (ao_list, sep) in self {
            let run_async = matches!(sep, ast::SeparatorOperator::Async);

            if run_async {
                todo!("asynchronous execution")
            }

            result = ao_list.first.execute(context)?;

            for next_ao in &ao_list.additional {
                let (is_and, pipeline) = match next_ao {
                    ast::AndOr::And(p) => (true, p),
                    ast::AndOr::Or(p) => (false, p),
                };

                if is_and {
                    if !result.is_success() {
                        break;
                    }
                } else {
                    if result.is_success() {
                        break;
                    }
                }

                result = pipeline.execute(context)?;
            }
        }

        Ok(result)
    }
}

impl Execute for ast::Pipeline {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult> {
        //
        // TODO: handle bang
        // TODO: implement logic deciding when to abort
        // TODO: confirm whether exit code comes from first or last in pipeline
        //

        let mut pipeline_context = PipelineExecutionContext {
            context,
            current_pipeline_index: 0,
            pipeline_len: self.seq.len(),
            spawn_results: vec![],
        };

        for command in self.seq.iter() {
            let spawn_result = command.execute_in_pipeline(&mut pipeline_context)?;
            pipeline_context.spawn_results.push(spawn_result);

            pipeline_context.current_pipeline_index += 1;
        }

        let mut result = ExecutionResult { exit_code: 0 };

        for child in pipeline_context.spawn_results.into_iter() {
            match child {
                SpawnResult::SpawnedChild(child) => {
                    let output = child.wait_with_output()?;

                    // TODO: Confirm what to return if it was signaled.
                    result = ExecutionResult {
                        exit_code: output.status.code().unwrap_or(127),
                    };
                }
                SpawnResult::ImmediateExit(exit_code) => result = ExecutionResult { exit_code },
            }
        }

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

                let result = compound.execute(pipeline_context.context)?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
            // TODO: Need to execute in pipeline.
            ast::Command::Function(func) => {
                let result = func.execute(pipeline_context.context)?;
                Ok(SpawnResult::ImmediateExit(result.exit_code))
            }
        }
    }
}

enum WhileOrUtil {
    While,
    Util,
}

impl Execute for ast::CompoundCommand {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult> {
        match self {
            ast::CompoundCommand::BraceGroup(g) => g.execute(context),
            ast::CompoundCommand::Subshell(_) => todo!("subshell command"),
            ast::CompoundCommand::ForClause(f) => f.execute(context),
            ast::CompoundCommand::CaseClause(c) => c.execute(context),
            ast::CompoundCommand::IfClause(i) => i.execute(context),
            ast::CompoundCommand::WhileClause(w) => (WhileOrUtil::While, w).execute(context),
            ast::CompoundCommand::UntilClause(u) => (WhileOrUtil::Util, u).execute(context),
        }
    }
}

impl Execute for ast::ForClauseCommand {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult> {
        let mut result = ExecutionResult { exit_code: 0 };

        if let Some(vs) = &self.values {
            for value in vs {
                // Update the variable.
                context
                    .parameters
                    .insert(self.variable_name.clone(), value.clone());

                result = self.body.execute(context)?;
            }
        }

        Ok(result)
    }
}

impl Execute for ast::CaseClauseCommand {
    fn execute(&self, _context: &mut ExecutionContext) -> Result<ExecutionResult> {
        todo!("execute case clause command")
    }
}

impl Execute for ast::IfClauseCommand {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult> {
        let condition = self.condition.execute(context)?;

        if condition.is_success() {
            return self.then.execute(context);
        }

        if let Some(elses) = &self.elses {
            for else_clause in elses {
                match &else_clause.condition {
                    Some(else_condition) => {
                        let else_condition_result = else_condition.execute(context)?;
                        if else_condition_result.is_success() {
                            return else_clause.body.execute(context);
                        }
                    }
                    None => {
                        return else_clause.body.execute(context);
                    }
                }
            }
        }

        return Ok(ExecutionResult { exit_code: 0 });
    }
}

impl Execute for (WhileOrUtil, &ast::WhileClauseCommand) {
    fn execute(&self, _context: &mut ExecutionContext) -> Result<ExecutionResult> {
        todo!("execute while clause command")
    }
}

impl Execute for ast::FunctionDefinition {
    fn execute(&self, context: &mut ExecutionContext) -> Result<ExecutionResult> {
        //
        // TODO: confirm whether defining a function resets the last execution.
        //

        context.funcs.insert(self.fname.clone(), self.clone());
        Ok(ExecutionResult { exit_code: 0 })
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
            .map(|a| expand_word(context.context, a))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        //
        // 3. Redirections shall be performed.
        //

        if redirects.len() > 0 {
            //
            // TODO: handle redirects
            //
            error!(
                "UNIMPLEMENTED: simple command redirects not implemented: {:?}",
                self
            );
        }

        //
        // 4. Each variable assignment shall be expanded for tilde expansion, parameter
        // expansion, command substitution, arithmetic expansion, and quote removal
        // prior to assigning the value.
        //

        let env_vars: Vec<_> = env_vars
            .iter()
            .map(|(n, v)| {
                let expanded_value = expand_word(context.context, v)?;
                Ok((n.clone(), expanded_value))
            })
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        if let Some(cmd_name) = &self.word_or_name {
            let mut cmd_name = cmd_name.to_owned();

            //
            // TODO: Reevaluate if this is an appropriate place to handle aliases.
            //
            if let Some(alias_value) = context.context.aliases.get(&cmd_name) {
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
                } else if let Some(function_definition) = context.context.funcs.get(&cmd_name) {
                    // Strip the function name off args.
                    invoke_shell_function(function_definition, &args[1..], &env_vars)
                } else if let Some(builtin) = builtins::BUILTINS.get(cmd_name.as_str()) {
                    execute_builtin_command(builtin, context, &args, &env_vars)
                } else {
                    // Strip the command name off args.
                    execute_external_command(context, cmd_name.as_ref(), &args[1..], &env_vars)
                }
            } else {
                // Strip the command name off args.
                execute_external_command(context, cmd_name.as_ref(), &args[1..], &env_vars)
            }
        } else {
            //
            // This must just be an assignment.
            //

            for (name, value) in env_vars {
                // TODO: Handle readonly variables.
                context
                    .context
                    .parameters
                    .insert(name.clone(), value.clone());
            }

            Ok(SpawnResult::ImmediateExit(0))
        }
    }
}

fn execute_external_command(
    context: &mut PipelineExecutionContext,
    cmd_name: &str,
    args: &[String],
    env_vars: &Vec<(String, String)>,
) -> Result<SpawnResult> {
    let mut cmd = std::process::Command::new(cmd_name);
    for arg in args {
        cmd.arg(arg);
    }

    for (name, value) in env_vars {
        cmd.env(name, value);
    }

    // See if we need to set up piping.
    // TODO: Handle stderr/other redirects/etc.
    if context.pipeline_len > 1 {
        if context.current_pipeline_index > 0 {
            // Find the stdout from the preceding process.
            if let Some(mut preceding_result) = context.spawn_results.pop() {
                match &mut preceding_result {
                    SpawnResult::SpawnedChild(child) => {
                        // Set up stdin of this process to take stdout of the preceding process.
                        cmd.stdin(std::process::Stdio::from(child.stdout.take().unwrap()));
                    }
                    SpawnResult::ImmediateExit(_code) => {
                        return Err(anyhow::anyhow!("Unable to retrieve piped command output"));
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

    match cmd.spawn() {
        Ok(child) => Ok(SpawnResult::SpawnedChild(child)),
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
    _env_vars: &Vec<(String, String)>,
) -> Result<SpawnResult> {
    let args: Vec<_> = args.iter().map(AsRef::as_ref).collect();
    let mut builtin_context = builtin::BuiltinExecutionContext {
        context: &mut context.context,
        builtin_name: args[0],
    };
    let builtin_result = builtin(&mut builtin_context, args.as_slice())?;

    let exit_code = match builtin_result.exit_code {
        builtin::BuiltinExitCode::Success => 0,
        builtin::BuiltinExitCode::InvalidUsage => 2,
        builtin::BuiltinExitCode::Unimplemented => 99,
        builtin::BuiltinExitCode::Custom(code) => code,
    };

    Ok(SpawnResult::ImmediateExit(exit_code))
}

fn invoke_shell_function(
    _function_definition: &ast::FunctionDefinition,
    _args: &[String],
    _env_vars: &Vec<(String, String)>,
) -> Result<SpawnResult> {
    log::error!("UNIMPLEMENTED: invoke shell function");
    Ok(SpawnResult::ImmediateExit(99))
}

fn expand_word(context: &ExecutionContext, word: &str) -> Result<String> {
    let expander = WordExpander::new(context);
    expander.expand(word)
}
