//! The `enable` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(EnableCommand);

use brush_core::ExecutionResult;
use brush_core::error;
use itertools::Itertools;
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &EnableCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut result = ExecutionResult::success();

    if command.shared_object_path.is_some() {
        return error::unimp("enable -f");
    }
    if command.remove_loaded_builtin {
        return error::unimp("enable -d");
    }

    if !command.names.is_empty() {
        for name in &command.names {
            if let Some(builtin) = context.shell.builtin_mut(name) {
                builtin.disabled = command.disable;
            } else {
                writeln!(context.stderr(), "{name}: not a shell builtin")?;
                result = ExecutionResult::general_error();
            }
        }
    } else {
        let builtins: Vec<_> = context
            .shell
            .builtins()
            .iter()
            .sorted_by_key(|(name, _reg)| *name)
            .collect();

        for (builtin_name, builtin) in builtins {
            if command.disable {
                if !builtin.disabled {
                    continue;
                }
            } else if command.print_list {
                if builtin.disabled {
                    continue;
                }
            }

            if command.special_only && !builtin.special_builtin {
                continue;
            }

            let prefix = if builtin.disabled { "-n " } else { "" };

            writeln!(context.stdout(), "enable {prefix}{builtin_name}")?;
        }
    }

    Ok(result)
}
