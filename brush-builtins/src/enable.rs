use brush_core::ExecutionResult;
use itertools::Itertools;
use std::io::Write;
use usage::Cli;

use brush_core::builtins;
use brush_core::error;

/// Enable, disable, or display built-in commands.
#[derive(Cli)]
#[usage(bin = "enable", unknown_flags = "error", args_override_self = false)]
pub(crate) struct EnableCommand {
    /// Print a list of built-in commands.
    #[usage(short = 'a')]
    print_list: bool,

    /// Disables the specified built-in commands.
    #[usage(short = 'n')]
    disable: bool,

    /// Print a list of built-in commands with reusable output.
    #[usage(short = 'p')]
    print_reusably: bool,

    /// Only operate on special built-in commands.
    #[usage(short = 's')]
    special_only: bool,

    /// Path to a shared object from which built-in commands will be loaded.
    #[usage(short = 'f', value_name = "PATH")]
    shared_object_path: Option<String>,

    /// Remove the built-in commands loaded from the indicated object path.
    #[usage(short = 'd')]
    remove_loaded_builtin: bool,

    /// Names of built-in commands to operate on.
    names: Vec<String>,
}

brush_builtin_usage::usage_builtin!(EnableCommand);

impl builtins::Command for EnableCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();

        if self.shared_object_path.is_some() {
            return error::unimp("enable -f");
        }
        if self.remove_loaded_builtin {
            return error::unimp("enable -d");
        }

        if !self.names.is_empty() {
            for name in &self.names {
                if let Some(builtin) = context.shell.builtin_mut(name) {
                    builtin.set_disabled(self.disable);
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
                if self.disable {
                    if !builtin.is_disabled() {
                        continue;
                    }
                } else if self.print_list {
                    if builtin.is_disabled() {
                        continue;
                    }
                }

                if self.special_only && !builtin.is_special() {
                    continue;
                }

                let prefix = if builtin.is_disabled() { "-n " } else { "" };

                writeln!(context.stdout(), "enable {prefix}{builtin_name}")?;
            }
        }

        Ok(result)
    }
}
