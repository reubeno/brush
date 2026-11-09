use itertools::Itertools;
use std::io::Write;

use brush_core::{ExecutionResult, builtins};

/// Manage aliases within the shell.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct AliasCommand {
    /// Print all defined aliases in a reusable format.
    #[arg(short = 'p')]
    print: bool,

    /// List of aliases to display or update.
    #[arg(positional, value_name = "name[=value]")]
    aliases: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(AliasCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for AliasCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut exit_code = ExecutionResult::success();

        // In POSIX mode, bash leaves out the `alias` prefix unless -p is given.
        let reusable = self.print || !context.shell.options().posix_mode;

        if self.print || self.aliases.is_empty() {
            // Aliases are stored unordered; bash lists them sorted by name.
            for (name, value) in context.shell.aliases().iter().sorted() {
                write_definition(context.stdout(), name, value, reusable)?;
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
                    write_definition(context.stdout(), alias, value, reusable)?;
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

/// Writes an alias definition as `alias` prints it: `name='value'`, or in the
/// reusable form `alias name='value'`, with `--` before a name that starts
/// with `-` so it can be read back.
fn write_definition(
    mut writer: impl Write,
    name: &str,
    value: &str,
    reusable: bool,
) -> std::io::Result<()> {
    if reusable {
        let separator = if name.starts_with('-') { "-- " } else { "" };
        write!(writer, "alias {separator}")?;
    }

    writeln!(writer, "{name}={}", brush_core::escape::single_quote(value))
}
