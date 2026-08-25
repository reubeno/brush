//! The `type_` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(TypeCommand);

use brush_core::ExecutionResult;
use std::io::Write;

use crate::lookup::{self, Resolved};

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &TypeCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut result = ExecutionResult::success();
    let options = lookup::Options {
        force_path_search: command.force_path_search,
        suppress_func_lookup: command.suppress_func_lookup,
        all_locations: command.all_locations,
        path_dirs: None,
    };

    for name in &command.names {
        let resolved_types = lookup::resolve(context.shell, name, &options);

        if resolved_types.is_empty() {
            if !command.type_only && !command.force_path_search && !command.show_path_only {
                writeln!(context.stderr(), "type: {name}: not found")?;
            }

            result = ExecutionResult::general_error();
            continue;
        }

        for resolved_type in resolved_types {
            if command.show_path_only && !matches!(resolved_type, Resolved::File { .. }) {
                // Do nothing.
            } else if command.type_only {
                match &resolved_type {
                    Resolved::Alias(_) => {
                        writeln!(context.stdout(), "alias")?;
                    }
                    Resolved::Keyword => {
                        writeln!(context.stdout(), "keyword")?;
                    }
                    Resolved::Function(_) => {
                        writeln!(context.stdout(), "function")?;
                    }
                    Resolved::Builtin => {
                        writeln!(context.stdout(), "builtin")?;
                    }
                    Resolved::File { path, .. } => {
                        if command.show_path_only || command.force_path_search {
                            writeln!(context.stdout(), "{}", path.to_string_lossy())?;
                        } else {
                            writeln!(context.stdout(), "file")?;
                        }
                    }
                }
            } else {
                match &resolved_type {
                    // When we're displaying all locations, we don't show hashed paths.
                    Resolved::File { hashed: true, .. }
                        if command.all_locations && !command.force_path_search => {}
                    Resolved::File { path, .. }
                        if command.show_path_only || command.force_path_search =>
                    {
                        writeln!(context.stdout(), "{}", path.to_string_lossy())?;
                    }
                    _ => lookup::describe(context.stdout(), name, &resolved_type)?,
                }
            }

            // If we only want the first, then break after the first.
            if !command.all_locations {
                break;
            }
        }
    }

    Ok(result)
}
