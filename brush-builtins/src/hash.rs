use brush_core::writeln_async;
use std::path::PathBuf;

use brush_core::{ExecutionResult, builtins};

#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct HashCommand {
    /// Display paths in a format usable for input.
    #[arg(short = 'l')]
    display_as_usable_input: bool,

    /// Remove all entries.
    #[arg(short = 'r')]
    remove_all: bool,

    /// The path to associate with the names.
    #[arg(short = 'p', value_name = "PATH")]
    path_to_use: Option<PathBuf>,

    /// Remove entries associated with the given names.
    #[arg(short = 'd')]
    remove: bool,

    /// Display the paths associated with the names.
    #[arg(short = 't')]
    display_paths: bool,

    /// Names to process.
    #[arg(positional)]
    names: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(HashCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for HashCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        let cmd = &context.command_name;

        // -d and -t need at least one name, and bash checks this before anything else.
        if (self.remove || self.display_paths) && self.names.is_empty() {
            let option = if self.remove { "-d" } else { "-t" };
            writeln!(
                context.stderr(),
                "{cmd}: {option}: option requires an argument"
            )?;
            return Ok(ExecutionResult::general_error());
        }

        if self.remove_all {
            context.shell.program_location_cache_mut().reset();
        } else if self.remove {
            for name in &self.names {
                if !context.shell.program_location_cache_mut().unset(name) {
                    writeln_async!(context.stderr_async(), "{cmd}: {name}: not found").await?;
                    result = ExecutionResult::general_error();
                }
            }
        } else if self.display_paths {
            for name in &self.names {
                if let Some(path) = context.shell.program_location_cache().get(name) {
                    if self.display_as_usable_input {
                        writeln_async!(
                            context.stdout_async(),
                            "builtin hash -p {} {name}",
                            path.to_string_lossy()
                        )
                        .await?;
                    } else {
                        let mut prefix = String::new();

                        if self.names.len() > 1 {
                            prefix.push_str(name.as_str());
                            prefix.push('\t');
                        }

                        writeln_async!(
                            context.stdout_async(),
                            "{prefix}{}",
                            path.to_string_lossy().as_ref()
                        )
                        .await?;
                    }
                } else {
                    writeln_async!(context.stderr_async(), "{cmd}: {name}: not found").await?;
                    result = ExecutionResult::general_error();
                }
            }
        } else if let Some(path) = &self.path_to_use {
            // The shell's working directory is its own state, not the process's, so a
            // relative path is resolved against it before it's inspected -- but reported as given.
            let is_dir = context.shell.absolute_path(path).is_dir();

            for name in &self.names {
                if is_dir {
                    writeln_async!(
                        context.stderr_async(),
                        "{cmd}: {}: Is a directory",
                        path.display()
                    )
                    .await?;
                    result = ExecutionResult::general_error();
                    continue;
                }

                context
                    .shell
                    .program_location_cache_mut()
                    .set(name, path.clone());
            }
        } else {
            for name in &self.names {
                // Remove from the cache if already hashed.
                let _ = context.shell.program_location_cache_mut().unset(name);

                // Names with slashes are accepted silently
                if name.contains('/') {
                    continue;
                }

                // Hash the path
                if context
                    .shell
                    .find_first_executable_in_path_using_cache(name)
                    .is_none()
                {
                    writeln_async!(context.stderr_async(), "{cmd}: {name}: not found").await?;
                    result = ExecutionResult::general_error();
                }
            }
        }

        Ok(result)
    }
}
