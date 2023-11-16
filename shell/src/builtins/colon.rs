use anyhow::Result;

use crate::context::{BuiltinExitCode, BuiltinResult, ExecutionContext};

pub(crate) fn builtin_colon(
    _context: &mut ExecutionContext,
    _args: &[&str],
) -> Result<BuiltinResult> {
    Ok(BuiltinResult {
        exit_code: BuiltinExitCode::Success,
    })
}
