//! The `hash` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(HashCommand);

use brush_core::ExecutionResult;
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &HashCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut result = ExecutionResult::success();

    if command.remove_all {
        context.shell.program_location_cache_mut().reset();
    } else if command.remove {
        for name in &command.names {
            if !context.shell.program_location_cache_mut().unset(name) {
                writeln!(context.stderr(), "{name}: not found")?;
                result = ExecutionResult::general_error();
            }
        }
    } else if command.display_paths {
        for name in &command.names {
            if let Some(path) = context.shell.program_location_cache().get(name) {
                if command.display_as_usable_input {
                    writeln!(
                        context.stdout(),
                        "builtin hash -p {} {name}",
                        path.to_string_lossy()
                    )?;
                } else {
                    let mut prefix = String::new();

                    if command.names.len() > 1 {
                        prefix.push_str(name.as_str());
                        prefix.push('\t');
                    }

                    writeln!(
                        context.stdout(),
                        "{prefix}{}",
                        path.to_string_lossy().as_ref()
                    )?;
                }
            } else {
                writeln!(context.stderr(), "{name}: not found")?;
                result = ExecutionResult::general_error();
            }
        }
    } else if let Some(path) = &command.path_to_use {
        for name in &command.names {
            context
                .shell
                .program_location_cache_mut()
                .set(name, path.clone());
        }
    } else {
        for name in &command.names {
            // Remove from the cache if already hashed.
            let _ = context.shell.program_location_cache_mut().unset(name);

            // Names with slashes are accepted silently
            if name.contains('/') {
                continue;
            }

            // Hash the path
            if context
                .shell
                .find_first_executable_in_path_using_cache(name)
                .is_none()
            {
                writeln!(context.stderr(), "{name}: not found")?;
                result = ExecutionResult::general_error();
            }
        }
    }

    Ok(result)
}
