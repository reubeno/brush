use anyhow::Result;

use crate::context::{BuiltinExitCode, BuiltinResult, ExecutionContext};

pub(crate) fn builtin_dot(_context: &mut ExecutionContext, args: &[&str]) -> Result<BuiltinResult> {
    if args.len() != 1 {
        log::error!("UNIMPLEMENTED: dot builtin with multiple args: {:?}", args);
        return Ok(BuiltinResult {
            exit_code: BuiltinExitCode::Unimplemented,
        });
    }

    //
    // TODO: Handle trap inheritance.
    //

    let script_path = args[0];
    log::error!("UNIMPLEMENTED: source {}", script_path);
    Ok(BuiltinResult {
        exit_code: BuiltinExitCode::Unimplemented,
    })
}
