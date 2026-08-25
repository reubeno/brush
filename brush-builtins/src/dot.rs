//! The `dot` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(DotCommand);

use std::path::Path;

async fn execute<SE: brush_core::ShellExtensions>(
    command: &DotCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    // TODO(dot): Handle trap inheritance.
    context
        .shell
        .source_script(
            Path::new(&command.script_path),
            command.script_args.iter(),
            &context.params,
        )
        .await
}
