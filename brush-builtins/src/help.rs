//! The `help` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(HelpCommand);

use brush_core::{ExecutionResult, builtins};
use itertools::Itertools;
use std::io::Write;

impl HelpCommand {
    fn display_general_help(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<(), brush_core::Error> {
        const COLUMN_COUNT: usize = 3;

        if let Some(display_str) = context.shell.product_display_str() {
            writeln!(context.stdout(), "{display_str}\n")?;
        }

        writeln!(
            context.stdout(),
            "The following commands are implemented as shell built-ins:"
        )?;

        let builtins = get_builtins_sorted_by_name(context);
        let items_per_column = builtins.len().div_ceil(COLUMN_COUNT);

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

    /// Displays help for the builtins matching `topic_pattern`, returning whether
    /// at least one topic matched.
    fn display_help_for_topic_pattern(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        topic_pattern: &str,
    ) -> Result<bool, brush_core::Error> {
        let pattern = brush_core::patterns::Pattern::from(topic_pattern)
            .set_extended_globbing(context.shell.options().extended_globbing)
            .set_case_insensitive(context.shell.options().case_insensitive_pathname_expansion);

        let mut matched = false;
        for (builtin_name, builtin_registration) in get_builtins_sorted_by_name(context) {
            if pattern.exactly_matches(builtin_name.as_str())? {
                self.display_help_for_builtin(
                    context,
                    builtin_name.as_str(),
                    builtin_registration,
                )?;
                matched = true;
            }
        }

        if !matched {
            writeln!(context.stderr(), "No help topics match '{topic_pattern}'")?;
        }

        Ok(matched)
    }

    fn display_help_for_builtin<SE: brush_core::ShellExtensions>(
        &self,
        context: &brush_core::ExecutionContext<'_, SE>,
        name: &str,
        registration: &builtins::Registration<SE>,
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

        let Some(mut stdout) = context.try_fd(brush_core::openfiles::OpenFiles::STDOUT_FD) else {
            // If there's no stdout, nothing to do.
            return Ok(());
        };

        // For now, we assume colorized output if stdout is a terminal.
        let options = builtins::ContentOptions {
            colorized: stdout.is_terminal(),
        };

        let content = (registration.content_func)(name, content_type, &options)?;

        write!(stdout, "{content}")?;
        stdout.flush()?;

        Ok(())
    }
}

pub(super) fn get_builtins_sorted_by_name<'a, SE: brush_core::ShellExtensions>(
    context: &'a brush_core::ExecutionContext<'_, SE>,
) -> Vec<(&'a String, &'a builtins::Registration<SE>)> {
    context
        .shell
        .builtins()
        .iter()
        .sorted_by_key(|(name, _)| *name)
        .collect()
}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &HelpCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.topic_patterns.is_empty() {
        HelpCommand::display_general_help(&context)?;
        return Ok(ExecutionResult::success());
    }

    // Match bash: succeed if at least one requested topic pattern matched
    // something; return a non-zero exit code only when none of them matched.
    let mut any_matched = false;
    for topic_pattern in &command.topic_patterns {
        if command.display_help_for_topic_pattern(&context, topic_pattern)? {
            any_matched = true;
        }
    }

    if any_matched {
        Ok(ExecutionResult::success())
    } else {
        Ok(ExecutionResult::general_error())
    }
}
