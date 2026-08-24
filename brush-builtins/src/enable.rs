use bpaf::Bpaf;
use itertools::Itertools;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, error};

/// Enable, disable, or display built-in commands.
#[derive(Bpaf)]
pub(crate) struct EnableCommand {
    /// Print a list of built-in commands.
    #[bpaf(short('a'))]
    print_list: bool,

    /// Disables the specified built-in commands.
    #[bpaf(short('n'))]
    disable: bool,

    /// Print a list of built-in commands with reusable output.
    #[bpaf(short('p'))]
    #[expect(dead_code)]
    print_reusably: bool,

    /// Only operate on special built-in commands.
    #[bpaf(short('s'))]
    special_only: bool,

    /// Path to a shared object from which built-in commands will be loaded.
    #[bpaf(short('f'), argument("PATH"))]
    shared_object_path: Option<String>,

    /// Remove the built-in commands loaded from the indicated object path.
    #[bpaf(short('d'))]
    remove_loaded_builtin: bool,

    /// Names of built-in commands to operate on.
    #[bpaf(positional("NAMES"))]
    names: Vec<String>,
}

impl builtins::Command for EnableCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        enable_command()
    }

    fn about() -> &'static str {
        "Enable, disable, or display built-in commands."
    }

    fn synopsis() -> &'static str {
        "[-adnps] [-f PATH] [NAMES]..."
    }

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
                    builtin.disabled = self.disable;
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
                    if !builtin.disabled {
                        continue;
                    }
                } else if self.print_list {
                    if builtin.disabled {
                        continue;
                    }
                }

                if self.special_only && !builtin.special_builtin {
                    continue;
                }

                let prefix = if builtin.disabled { "-n " } else { "" };

                writeln!(context.stdout(), "enable {prefix}{builtin_name}")?;
            }
        }

        Ok(result)
    }
}
