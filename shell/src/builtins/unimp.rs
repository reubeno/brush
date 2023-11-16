use crate::context::{BuiltinExitCode, BuiltinResult, ExecutionContext};

use anyhow::Result;

pub(crate) fn builtin_unimplemented(
    _context: &mut ExecutionContext,
    args: &[&str],
) -> Result<BuiltinResult> {
    log::error!("built-in unimplemented: {}", args[0]);
    Ok(BuiltinResult {
        exit_code: BuiltinExitCode::Unimplemented,
    })
}
