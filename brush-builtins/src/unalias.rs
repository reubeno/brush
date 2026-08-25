use std::io::Write;

use brush_core::{
    ExecutionResult,
    argmodel::{ArgSpec, PositionalSpec},
    builtins,
};

/// Unset a shell alias.
pub(crate) struct UnaliasCommand {
    remove_all: bool,
    aliases: Vec<String>,
}

const ID_REMOVE_ALL: &str = "remove_all";
const ID_ALIASES: &str = "aliases";

impl builtins::SpecCommand for UnaliasCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[ArgSpec::flag(
                ID_REMOVE_ALL,
                &['a'],
                &[],
                "Remove all aliases.",
            )],
            positionals: &[PositionalSpec::many(ID_ALIASES, "ALIASES")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            remove_all: values.flag(ID_REMOVE_ALL),
            aliases: values.positional_values(ID_ALIASES).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Unset a shell alias."
    }

    fn synopsis() -> &'static str {
        "[-a] [ALIASES]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut exit_code = ExecutionResult::success();

        if self.remove_all {
            context.shell.aliases_mut().clear();
        } else {
            for alias in &self.aliases {
                if context.shell.aliases_mut().remove(alias).is_none() {
                    writeln!(
                        context.stderr(),
                        "{}: {}: not found",
                        context.command_name,
                        alias
                    )?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        }

        Ok(exit_code)
    }
}
