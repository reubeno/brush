use crate::{builtin, commands, error};
use clap::Parser;
use itertools::Itertools;
use std::io::Write;

/// Display command help.
#[derive(Parser)]
pub(crate) struct HelpCommand {
    /// Display a short description for the commands.
    #[arg(short = 'd')]
    short_description: bool,

    /// Display a man-style page of documentation for the commands.
    #[arg(short = 'm')]
    man_page_style: bool,

    /// Display a short usage summary for the commands.
    #[arg(short = 's')]
    short_usage: bool,

    /// Patterns of topics to display help for.
    topic_patterns: Vec<String>,
}

#[async_trait::async_trait]
impl builtin::Command for HelpCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        if self.topic_patterns.is_empty() {
            Self::display_general_help(&context)?;
        } else {
            for topic_pattern in &self.topic_patterns {
                self.display_help_for_topic_pattern(&context, topic_pattern)?;
            }
        }

        Ok(builtin::ExitCode::Success)
    }
}

impl HelpCommand {
    fn display_general_help(
        context: &commands::ExecutionContext<'_>,
    ) -> Result<(), crate::error::Error> {
        const COLUMN_COUNT: usize = 3;

        if let Some(display_str) = &context.shell.shell_product_display_str {
            writeln!(context.stdout(), "{display_str}\n")?;
        }

        writeln!(
            context.stdout(),
            "The following commands are implemented as shell built-ins:"
        )?;

        let builtins = get_builtins_sorted_by_name(context);
        let items_per_column = (builtins.len() + COLUMN_COUNT - 1) / COLUMN_COUNT;

        for i in 0..items_per_column {
            for j in 0..COLUMN_COUNT {
                if let Some((name, builtin)) = builtins.get(i + j * items_per_column) {
                    let prefix = if builtin.disabled { "*" } else { " " };
                    write!(context.stdout(), "  {prefix}{name:<20}")?; // adjust 20 to the desired
                                                                       // column width
                }
            }
            writeln!(context.stdout())?;
        }

        Ok(())
    }

    fn display_help_for_topic_pattern(
        &self,
        context: &commands::ExecutionContext<'_>,
        topic_pattern: &str,
    ) -> Result<(), crate::error::Error> {
        let pattern = crate::patterns::Pattern::from(topic_pattern);

        let mut found_count = 0;
        for (builtin_name, builtin_registration) in get_builtins_sorted_by_name(context) {
            if pattern.exactly_matches(builtin_name.as_str(), false)? {
                self.display_help_for_builtin(
                    context,
                    builtin_name.as_str(),
                    builtin_registration,
                )?;
                found_count += 1;
            }
        }

        if found_count == 0 {
            writeln!(context.stderr(), "No help topics match '{topic_pattern}'")?;
        }

        Ok(())
    }

    fn display_help_for_builtin(
        &self,
        context: &commands::ExecutionContext<'_>,
        name: &str,
        registration: &builtin::Registration,
    ) -> Result<(), error::Error> {
        let content_type = if self.short_description {
            builtin::ContentType::ShortDescription
        } else if self.man_page_style {
            builtin::ContentType::ManPage
        } else if self.short_usage {
            builtin::ContentType::ShortUsage
        } else {
            builtin::ContentType::DetailedHelp
        };

        let content = (registration.content_func)(name, content_type)?;

        write!(context.stdout(), "{content}")?;
        context.stdout().flush()?;

        Ok(())
    }
}

fn get_builtins_sorted_by_name<'a>(
    context: &'a commands::ExecutionContext<'_>,
) -> Vec<(&'a String, &'a builtin::Registration)> {
    context
        .shell
        .builtins
        .iter()
        .sorted_by_key(|(name, _)| *name)
        .collect()
}
