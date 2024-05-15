use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use clap::Parser;
use std::io::Write;

#[derive(Parser)]
pub(crate) struct HelpCommand {
    #[arg(short = 'd')]
    short_description: bool,

    #[arg(short = 'm')]
    man_page_style: bool,

    #[arg(short = 's')]
    short_usage: bool,

    topic_patterns: Vec<String>,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[async_trait::async_trait]
impl BuiltinCommand for HelpCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.topic_patterns.is_empty() {
            Self::display_general_help(&context)?;
        } else {
            for topic_pattern in &self.topic_patterns {
                self.display_help_for_topic_pattern(&context, topic_pattern)?;
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}

impl HelpCommand {
    fn display_general_help(
        context: &crate::context::CommandExecutionContext<'_>,
    ) -> Result<(), crate::error::Error> {
        const COLUMN_COUNT: usize = 3;

        writeln!(context.stdout(), "brush version {VERSION}\n")?;

        writeln!(
            context.stdout(),
            "The following commands are implemented as shell built-ins:"
        )?;

        let builtin_names = context.shell.get_builtin_names();
        let items_per_column = (builtin_names.len() + COLUMN_COUNT - 1) / COLUMN_COUNT;

        for i in 0..items_per_column {
            for j in 0..COLUMN_COUNT {
                if let Some(name) = builtin_names.get(i + j * items_per_column) {
                    write!(context.stdout(), "  {name:<20}")?; // adjust 20 to the desired column width
                }
            }
            writeln!(context.stdout())?;
        }

        Ok(())
    }

    fn display_help_for_topic_pattern(
        &self,
        context: &crate::context::CommandExecutionContext<'_>,
        topic_pattern: &str,
    ) -> Result<(), crate::error::Error> {
        let pattern = crate::patterns::Pattern::from(topic_pattern);

        let mut found_count = 0;
        for builtin_name in context.shell.get_builtin_names() {
            if pattern.exactly_matches(builtin_name.as_str(), false)? {
                self.display_help_for_builtin(context, builtin_name.as_str())?;
                found_count += 1;
            }
        }

        if found_count == 0 {
            writeln!(context.stderr(), "No help topics match '{topic_pattern}'")?;
        }

        Ok(())
    }

    #[allow(clippy::unused_self)]
    fn display_help_for_builtin(
        &self,
        context: &crate::context::CommandExecutionContext<'_>,
        builtin_name: &str,
    ) -> Result<(), crate::error::Error> {
        // TODO: implement the rest of this
        writeln!(context.stdout(), "Help for '{builtin_name}'")?;
        Ok(())
    }
}
