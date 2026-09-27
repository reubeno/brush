use brush_core::{ExecutionResult, builtins};
use clap::Parser;
use std::io::Write;

/// (*EXPERIMENTAL*) Serializes the current shell state to JSON and writes it to stdout.
/// Beware that the serialized state may include sensitive information, such as any
/// secrets stored in shell variables or referenced in command history.
#[derive(Parser)]
pub(crate) struct SaveCommand {}

pub(crate) fn registration<SE: brush_core::ShellExtensions>() -> builtins::Registration<SE>
where
    brush_core::Shell<SE>: serde::Serialize,
{
    builtins::Registration {
        execute_func: execute::<SE>,
        content_func: builtins::get_parser_content::<SaveCommand>,
        disabled: false,
        special_builtin: false,
        declaration_builtin: false,
    }
}

fn execute<SE: brush_core::ShellExtensions>(
    context: brush_core::ExecutionContext<'_, SE>,
    args: Vec<brush_core::CommandArg>,
) -> builtins::BoxFuture<'_, Result<ExecutionResult, brush_core::Error>>
where
    brush_core::Shell<SE>: serde::Serialize,
{
    Box::pin(async move {
        if let Err(error) = SaveCommand::try_parse_from(args.into_iter().map(|arg| arg.to_string()))
        {
            let _ = writeln!(context.stderr(), "{error}");
            return Ok(brush_core::ExecutionExitCode::InvalidUsage.into());
        }
        let serialized_str = serde_json::to_string(&context.shell).map_err(|e| {
            brush_core::Error::from(brush_core::ErrorKind::InternalError(e.to_string()))
        })?;

        writeln!(context.stdout(), "{serialized_str}")?;

        Ok(ExecutionResult::success())
    })
}
