//! The `caller` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(CallerCommand);

use brush_core::{ExecutionResult, callstack};
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &CallerCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let stack = context.shell.call_stack();

    // See how far back we need to look. Frame N represents the Nth caller
    // (e.g., 0 = immediate caller, 1 = caller's caller, etc.).
    let expr = command.expr.unwrap_or(0);

    // Get all frames into a vector we can easily index into.
    let frames: Vec<_> = stack
        .iter()
        .filter(|frame| frame.frame_type.is_function() || frame.frame_type.is_script())
        .collect();

    // Look for the last-known location in the parent of frame N.
    let Some(calling_frame) = frames.get(expr + 1) else {
        return Ok(ExecutionResult::general_error());
    };

    let line = calling_frame.current_line().unwrap_or(1);
    let filename = &calling_frame.source_info.source;

    // When the expr is provided, we display "LINE FUNCTION_NAME FILENAME"
    // When the expr is omitted, we only display "LINE FILENAME"
    if command.expr.is_some() {
        let function_name = match &calling_frame.frame_type {
            callstack::FrameType::Function(func_call) => func_call.name(),
            callstack::FrameType::Script(..) => "source".into(),
            _ => "".into(),
        };

        writeln!(context.stdout(), "{line} {function_name} {filename}")?;
    } else {
        writeln!(context.stdout(), "{line} {filename}")?;
    }

    Ok(ExecutionResult::success())
}
