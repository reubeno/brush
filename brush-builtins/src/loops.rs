//! What `break` and `continue` share: leaving the loops that enclose them.

use std::io::Write;

use brush_core::{ExecutionControlFlow, ExecutionResult};

/// Runs `break` or `continue` (`name`) with its loop `count`: leaves as many
/// of the enclosing loops as `count` says, with `leave` giving the control
/// flow that does it.
///
/// Outside a loop it does nothing (reporting so, outside POSIX mode). A count
/// past the outermost loop stops there, and a count below 1 is reported and
/// leaves every loop.
pub(crate) fn leave<SE: brush_core::ShellExtensions>(
    context: &brush_core::ExecutionContext<'_, SE>,
    name: &str,
    count: i8,
    leave: fn(usize) -> ExecutionControlFlow,
) -> ExecutionResult {
    let loops = context.shell.loop_depth();
    if loops == 0 {
        if !context.shell.options().posix_mode {
            // Diagnostics are best-effort: a closed stderr doesn't change what we do.
            let _ = writeln!(
                context.stderr(),
                "{name}: only meaningful in a `for', `while', or `until' loop"
            );
        }
        return ExecutionResult::success();
    }

    let mut result = ExecutionResult::success();
    if let Ok(count @ 1..) = usize::try_from(count) {
        result.next_control_flow = leave(count.min(loops) - 1);
    } else {
        let _ = writeln!(context.stderr(), "{name}: {count}: loop count out of range");
        result = ExecutionResult::general_error();
        result.next_control_flow = ExecutionControlFlow::BreakLoop { levels: loops - 1 };
    }

    result
}
