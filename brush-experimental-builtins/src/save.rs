use brush_core::{ExecutionResult, builtins};
use clap::Parser;
use std::io::Write;

/// (*EXPERIMENTAL*) Serializes the current shell state to JSON and writes it to stdout.
/// Beware that the serialized state may include sensitive information, such as any
/// secrets stored in shell variables or referenced in command history.
#[derive(Parser)]
pub(crate) struct SaveCommand {}

impl builtins::Command for SaveCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let serialized_str = serde_json::to_string(&context.shell).map_err(|e| {
            brush_core::Error::from(brush_core::ErrorKind::InternalError(e.to_string()))
        })?;

        writeln!(context.stdout(), "{serialized_str}")?;

        Ok(ExecutionResult::success())
    }
}
