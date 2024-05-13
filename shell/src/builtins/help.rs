use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use clap::Parser;
use std::io::Write;

#[derive(Parser)]
pub(crate) struct HelpCommand {}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[async_trait::async_trait]
impl BuiltinCommand for HelpCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        writeln!(context.stdout(), "brush version {VERSION}\n")?;

        writeln!(
            context.stdout(),
            "The following commands are implemented as shell built-ins:"
        )?;

        let builtin_names: Vec<_> = crate::builtins::get_all_builtin_names();

        const COLUMN_COUNT: usize = 3;

        let items_per_column = (builtin_names.len() + COLUMN_COUNT - 1) / COLUMN_COUNT;

        for i in 0..items_per_column {
            for j in 0..COLUMN_COUNT {
                if let Some(name) = builtin_names.get(i + j * items_per_column) {
                    write!(context.stdout(), "  {name:<20}")?; // adjust 20 to the desired column width
                }
            }
            writeln!(context.stdout())?;
        }

        Ok(BuiltinExitCode::Success)
    }
}
