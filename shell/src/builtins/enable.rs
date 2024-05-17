use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use crate::error;

#[derive(Parser)]
pub(crate) struct EnableCommand {
    #[arg(short = 'a')]
    print_list: bool,

    #[arg(short = 'n')]
    disable: bool,

    #[arg(short = 'p')]
    print_reusably: bool,

    #[arg(short = 's')]
    special_only: bool,

    #[arg(short = 'f')]
    shared_object_path: Option<String>,

    #[arg(short = 'd')]
    remove_loaded_builtin: bool,

    names: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for EnableCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut result = BuiltinExitCode::Success;

        if self.shared_object_path.is_some() {
            return error::unimp("enable -f");
        }
        if self.remove_loaded_builtin {
            return error::unimp("enable -d");
        }

        if !self.names.is_empty() {
            for name in &self.names {
                if let Some(builtin) = context.shell.builtins.get_mut(name) {
                    builtin.disabled = self.disable;
                } else {
                    writeln!(context.stderr(), "{name}: not a shell builtin")?;
                    result = BuiltinExitCode::Custom(1);
                }
            }
        } else {
            let builtins: Vec<_> = context
                .shell
                .builtins
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
