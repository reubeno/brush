use itertools::Itertools;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, error};

/// Enable, disable, or display built-in commands.
pub(crate) struct EnableCommand {
    print_list: bool,
    disable: bool,
    #[expect(dead_code)]
    print_reusably: bool,
    special_only: bool,
    shared_object_path: Option<String>,
    remove_loaded_builtin: bool,
    names: Vec<String>,
}

const ID_PRINT_LIST: &str = "print_list";
const ID_DISABLE: &str = "disable";
const ID_PRINT_REUSABLY: &str = "print_reusably";
const ID_SPECIAL_ONLY: &str = "special_only";
const ID_SHARED_OBJECT_PATH: &str = "shared_object_path";
const ID_REMOVE_LOADED_BUILTIN: &str = "remove_loaded_builtin";
const ID_NAMES: &str = "names";

impl builtins::SpecCommand for EnableCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_PRINT_LIST,
            &['a'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Print a list of built-in commands.",
        )
        .arg(
            ID_DISABLE,
            &['n'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Disables the specified built-in commands.",
        )
        .arg(
            ID_PRINT_REUSABLY,
            &['p'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Print a list of built-in commands with reusable output.",
        )
        .arg(
            ID_SPECIAL_ONLY,
            &['s'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Only operate on special built-in commands.",
        )
        .arg(
            ID_SHARED_OBJECT_PATH,
            &['f'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("PATH"),
            "Path to a shared object from which built-in commands will be loaded.",
        )
        .arg(
            ID_REMOVE_LOADED_BUILTIN,
            &['d'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Remove the built-in commands loaded from the indicated object path.",
        )
        .positional_many(ID_NAMES, "NAMES")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            print_list: matches.flag(ID_PRINT_LIST),
            disable: matches.flag(ID_DISABLE),
            print_reusably: matches.flag(ID_PRINT_REUSABLY),
            special_only: matches.flag(ID_SPECIAL_ONLY),
            shared_object_path: matches.value(ID_SHARED_OBJECT_PATH).map(ToOwned::to_owned),
            remove_loaded_builtin: matches.flag(ID_REMOVE_LOADED_BUILTIN),
            names: matches.values(ID_NAMES).to_vec(),
        })
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
