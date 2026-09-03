use clap::Parser;
use std::io::Write;
use std::path::PathBuf;

use brush_core::{ExecutionResult, builtins, commands, sys};

use crate::lookup::{self, Resolved};
use crate::write_alias_definition;

/// Directly invokes an external command, without going through typical search order.
#[derive(Default, Parser)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    #[arg(short = 'p')]
    pub use_default_path: bool,

    /// Display a short description of the command.
    #[arg(short = 'v', overrides_with = "print_verbose_description")]
    pub print_description: bool,

    /// Display a more verbose description of the command.
    #[arg(short = 'V', overrides_with = "print_description")]
    pub print_verbose_description: bool,

    /// Command and arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command_and_args: Vec<String>,
}

impl CommandCommand {
    fn command(&self) -> Option<&str> {
        self.command_and_args.first().map(|s| s.as_str())
    }

    fn path_dirs(&self) -> Option<Vec<PathBuf>> {
        self.use_default_path
            .then(sys::fs::get_default_standard_utils_paths)
    }

    /// Describes every name given, as `-v`/`-V` do; succeeds if any name was found.
    fn describe_names<SE: brush_core::ShellExtensions>(
        &self,
        context: &brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        // With no names to look up there is nothing to fail to find, so this still succeeds.
        if self.command_and_args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let options = lookup::Options {
            path_dirs: self.path_dirs(),
            ..Default::default()
        };

        let mut any_found = false;
        for name in &self.command_and_args {
            let Some(mut found) = lookup::resolve(context.shell, name, &options)
                .into_iter()
                .next()
            else {
                if self.print_verbose_description {
                    writeln!(context.stderr(), "command: {name}: not found")?;
                }
                continue;
            };

            any_found = true;
            if self.print_description {
                // Display in a form that could be reused as shell input.
                match &found {
                    Resolved::Alias(target) => {
                        write_alias_definition(context.stdout(), name, target)?;
                    }
                    Resolved::Keyword | Resolved::Function(_) | Resolved::Builtin => {
                        writeln!(context.stdout(), "{name}")?;
                    }
                    Resolved::File { path, .. } => {
                        writeln!(context.stdout(), "{}", path.to_string_lossy())?;
                    }
                }
            } else {
                // A command found by searching PATH is reported with an absolute path, even
                // when the PATH entry it came from was relative. Explicit paths and hashed
                // entries are reported exactly as they were given. Like bash, this only
                // prefixes the working directory; it doesn't otherwise normalize the path,
                // except to drop the leading `./` a `.` entry in PATH contributes.
                if let Resolved::File {
                    path,
                    hashed: false,
                } = &mut found
                    && !sys::fs::contains_path_separator(name)
                {
                    let relative = path.strip_prefix(".").unwrap_or(&*path);
                    *path = context.shell.absolute_path(relative);
                }

                lookup::describe(context.stdout(), name, &found)?;
            }
        }

        if any_found {
            Ok(ExecutionResult::success())
        } else {
            Ok(ExecutionResult::general_error())
        }
    }

    async fn execute_command(
        &self,
        mut context: brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        command_name: &str,
    ) -> Result<ExecutionResult, brush_core::Error> {
        command_name.clone_into(&mut context.command_name);
        let command_and_args = self
            .command_and_args
            .iter()
            .map(brush_core::CommandArg::from);

        let mut cmd = commands::SimpleCommand::new(
            commands::ShellForCommand::ParentShell(context.shell),
            context.params,
            context.command_name,
            command_and_args,
        );
        cmd.use_functions = false;
        cmd.path_dirs = self.path_dirs();

        let spawn_result = cmd.execute().await?;
        let wait_result = spawn_result.wait().await?;

        Ok(wait_result.into())
    }
}

impl builtins::Command for CommandCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.print_description || self.print_verbose_description {
            return self.describe_names(&context);
        }

        // Silently exit if no command was provided.
        let Some(command_name) = self.command() else {
            return Ok(ExecutionResult::success());
        };

        self.execute_command(context, command_name).await
    }
}
