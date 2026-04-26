use brush_core::{ExecutionResult, builtins};
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

impl builtins::Command for HelpCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // Buffer output for async write
        let mut output = Vec::new();

        if self.topic_patterns.is_empty() {
            Self::display_general_help(&context, &mut output)?;
        } else {
            for topic_pattern in &self.topic_patterns {
                self.display_help_for_topic_pattern(&context, topic_pattern, &mut output)?;
            }
        }

        // Write output async
        if !output.is_empty() {
            if let Some(mut stdout) = context.stdout() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            }
        }

        Ok(ExecutionResult::success())
    }
}

impl HelpCommand {
    fn display_general_help(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        output: &mut Vec<u8>,
    ) -> Result<(), brush_core::Error> {
        const COLUMN_COUNT: usize = 3;

        if let Some(display_str) = context.shell.product_display_str() {
            writeln!(output, "{display_str}\n")?;
        }

        writeln!(
            output,
            "The following commands are implemented as shell built-ins:"
        )?;

        let builtins = get_builtins_sorted_by_name(context);
        let items_per_column = builtins.len().div_ceil(COLUMN_COUNT);

        for i in 0..items_per_column {
            for j in 0..COLUMN_COUNT {
                if let Some((name, builtin)) = builtins.get(i + j * items_per_column) {
                    let prefix = if builtin.disabled { "*" } else { " " };
                    write!(output, "  {prefix}{name:<20}")?;
                }
            }
            writeln!(output)?;
        }

        Ok(())
    }

    fn display_help_for_topic_pattern(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        topic_pattern: &str,
        output: &mut Vec<u8>,
    ) -> Result<(), brush_core::Error> {
        let pattern = brush_core::patterns::Pattern::from(topic_pattern)
            .set_extended_globbing(context.shell.options().extended_globbing)
            .set_case_insensitive(context.shell.options().case_insensitive_pathname_expansion);

        let mut found_count = 0;
        for (builtin_name, builtin_registration) in get_builtins_sorted_by_name(context) {
            if pattern.exactly_matches(builtin_name.as_str())? {
                self.display_help_for_builtin(
                    context,
                    builtin_name.as_str(),
                    builtin_registration,
                    output,
                )?;
                found_count += 1;
            }
        }

        if found_count == 0 {
            writeln!(output, "No help topics match '{topic_pattern}'")?;
        }

        Ok(())
    }

    fn display_help_for_builtin<SE: brush_core::ShellExtensions>(
        &self,
        context: &brush_core::ExecutionContext<'_, SE>,
        name: &str,
        registration: &builtins::Registration<SE>,
        output: &mut Vec<u8>,
    ) -> Result<(), brush_core::Error> {
        let content_type = if self.short_description {
            builtins::ContentType::ShortDescription
        } else if self.man_page_style {
            builtins::ContentType::ManPage
        } else if self.short_usage {
            builtins::ContentType::ShortUsage
        } else {
            builtins::ContentType::DetailedHelp
        };

        let Some(stdout) = context.try_fd(brush_core::openfiles::OpenFiles::STDOUT_FD) else {
            return Ok(());
        };

        let options = builtins::ContentOptions {
            colorized: stdout.is_terminal(),
        };

        let content = (registration.content_func)(name, content_type, &options)?;

        write!(output, "{content}")?;

        Ok(())
    }
}

fn get_builtins_sorted_by_name<'a, SE: brush_core::ShellExtensions>(
    context: &'a brush_core::ExecutionContext<'_, SE>,
) -> Vec<(&'a String, &'a builtins::Registration<SE>)> {
    context
        .shell
        .builtins()
        .iter()
        .sorted_by_key(|(name, _)| *name)
        .collect()
}
