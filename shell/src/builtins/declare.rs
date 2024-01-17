use anyhow::Result;
use clap::Parser;
use itertools::Itertools;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    env::{EnvironmentLookup, EnvironmentScope},
};

#[derive(Parser, Debug)]
pub(crate) struct DeclareCommand {
    #[arg(short = 'f')]
    function_names_or_defs_only: bool,

    #[arg(short = 'F')]
    function_names_only: bool,

    #[arg(short = 'g')]
    create_global: bool,

    #[arg(short = 'I')]
    locals_inherit_from_prev_scope: bool,

    #[arg(short = 'p')]
    print: bool,

    //
    // Attribute options
    //
    // TODO: allow + to be used to disable option
    //
    #[arg(short = 'a')]
    make_indexed_array: bool,

    #[arg(short = 'A')]
    make_associative_array: bool,

    #[arg(short = 'i')]
    make_integer: bool,

    #[arg(short = 'l')]
    lowercase_value_on_assignment: bool,

    #[arg(short = 'n')]
    make_nameref: bool,

    #[arg(short = 'r')]
    make_readonly: bool,

    #[arg(short = 't')]
    make_traced: bool,

    #[arg(short = 'u')]
    uppercase_value_on_assignment: bool,

    #[arg(short = 'x')]
    make_exported: bool,

    #[arg(name = "name[=value]")]
    names: Vec<String>,
}

#[allow(clippy::too_many_lines)]
#[async_trait::async_trait]
impl BuiltinCommand for DeclareCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        let called_as_local = context.builtin_name == "local";
        let create_var_local =
            called_as_local || (context.shell.in_function() && !self.create_global);

        // Note that we don't implement much.
        if self.function_names_or_defs_only {
            log::error!("UNIMPLEMENTED: declare -f: function names or definitions only");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.function_names_only {
            log::error!("UNIMPLEMENTED: declare -F: function names only");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.locals_inherit_from_prev_scope {
            log::error!("UNIMPLEMENTED: declare -I: locals inherit from previous scope");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.print {
            log::error!("UNIMPLEMENTED: declare -p: print");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if self.make_indexed_array {
            log::error!("UNIMPLEMENTED: declare -a: make indexed array");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.make_associative_array {
            log::error!("UNIMPLEMENTED: declare -A: make associative array");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.make_integer {
            log::error!("UNIMPLEMENTED: declare -i: make integer");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.lowercase_value_on_assignment {
            log::error!("UNIMPLEMENTED: declare -l: lowercase value on assignment");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.make_nameref {
            log::error!("UNIMPLEMENTED: declare -n: make nameref");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.make_readonly {
            log::error!("UNIMPLEMENTED: declare -r: make readonly");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.make_traced {
            log::error!("UNIMPLEMENTED: declare -t: make traced");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.uppercase_value_on_assignment {
            log::error!("UNIMPLEMENTED: declare -u: uppercase value on assignment");
            return Ok(BuiltinExitCode::Unimplemented);
        }
        if self.make_exported {
            log::error!("UNIMPLEMENTED: declare -x: make exported");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        let (names, plus_args): (Vec<_>, Vec<_>) =
            self.names.iter().partition(|name| !name.starts_with('+'));

        if !plus_args.is_empty() {
            log::error!("UNIMPLEMENTED: declare +: plus args used");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if !names.is_empty() {
            for entry in names {
                let (name, mut value) = entry.split_once('=').map_or_else(
                    || (entry.as_str(), None),
                    |(name, value)| (name, Some(value)),
                );

                // TODO: handle declaring without value for variable of different type.
                if value.is_none() {
                    value = Some("");
                }

                if create_var_local {
                    context.shell.env.update_or_add(
                        name,
                        value.unwrap(),
                        |_| Ok(()),
                        EnvironmentLookup::OnlyInCurrentLocal,
                        EnvironmentScope::Local,
                    )?;
                } else {
                    context.shell.env.update_or_add(
                        name,
                        value.unwrap(),
                        |_| Ok(()),
                        EnvironmentLookup::OnlyInGlobal,
                        EnvironmentScope::Global,
                    )?;
                }

                // TODO: update name with attributes
            }

            return Ok(BuiltinExitCode::Unimplemented);
        } else {
            // Dump variables.
            // TODO: Add annotations for variable type.
            for (name, variable) in context
                .shell
                .env
                .iter()
                .filter(|(_, v)| v.enumerable)
                .sorted_by_key(|v| v.0)
            {
                println!("{}={}", name, variable.value.format()?);
            }

            // TODO: dump functions
        }

        Ok(BuiltinExitCode::Success)
    }
}
