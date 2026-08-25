use std::io::Write;

use brush_core::{
    ExecutionResult,
    argmodel::{ArgSpec, PositionalSpec},
    builtins,
};

/// Manage aliases within the shell.
pub(crate) struct AliasCommand {
    print: bool,
    aliases: Vec<String>,
}

const ID_PRINT: &str = "print";
const ID_ALIASES: &str = "aliases";

impl builtins::SpecCommand for AliasCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[ArgSpec::flag(
                ID_PRINT,
                &['p'],
                &[],
                "Print all defined aliases in a reusable format.",
            )],
            positionals: &[PositionalSpec::many(ID_ALIASES, "name[=value]")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            print: values.flag(ID_PRINT),
            aliases: values.positional_values(ID_ALIASES).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Manage aliases within the shell."
    }

    fn synopsis() -> &'static str {
        "[-p] [name[=value]]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut exit_code = ExecutionResult::success();

        if self.print || self.aliases.is_empty() {
            for (name, value) in context.shell.aliases() {
                writeln!(context.stdout(), "alias {name}='{value}'")?;
            }
        } else {
            for alias in &self.aliases {
                if let Some((name, unexpanded_value)) = alias.split_once('=')
                    && !name.is_empty()
                {
                    context
                        .shell
                        .aliases_mut()
                        .insert(name.to_owned(), unexpanded_value.to_owned());
                } else if let Some(value) = context.shell.aliases().get(alias) {
                    writeln!(context.stdout(), "alias {alias}='{value}'")?;
                } else {
                    writeln!(
                        context.stderr(),
                        "{}: {alias}: not found",
                        context.command_name
                    )?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        }

        Ok(exit_code)
    }
}
