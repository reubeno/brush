use clap::Parser;
use std::{io::Write, path::PathBuf};

use brush_core::{ExecutionResult, builtins};

#[derive(Parser)]
pub(crate) struct HashCommand {
    /// Remove entries associated with the given names.
    #[arg(short = 'd')]
    remove: bool,

    /// Display paths in a format usable for input.
    #[arg(short = 'l')]
    display_as_usable_input: bool,

    /// The path to associate with the names.
    #[arg(short = 'p', value_name = "PATH")]
    path_to_use: Option<PathBuf>,

    /// Remove all entries.
    #[arg(short = 'r')]
    remove_all: bool,

    /// Display the paths associated with the names.
    #[arg(short = 't')]
    display_paths: bool,

    /// Names to process.
    names: Vec<String>,
}

impl builtins::Command for HashCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        let mut output = Vec::new();
        let mut stderr_output = Vec::new();

        if self.remove_all {
            context.shell.program_location_cache_mut().reset();
        } else if self.remove {
            for name in &self.names {
                if !context.shell.program_location_cache_mut().unset(name) {
                    writeln!(stderr_output, "{name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        } else if self.display_paths {
            for name in &self.names {
                if let Some(path) = context.shell.program_location_cache().get(name) {
                    if self.display_as_usable_input {
                        writeln!(output, "builtin hash -p {} {name}", path.to_string_lossy())?;
                    } else {
                        let mut prefix = String::new();

                        if self.names.len() > 1 {
                            prefix.push_str(name.as_str());
                            prefix.push('\t');
                        }

                        writeln!(output, "{prefix}{}", path.to_string_lossy().as_ref())?;
                    }
                } else {
                    writeln!(stderr_output, "{name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        } else if let Some(path) = &self.path_to_use {
            for name in &self.names {
                context
                    .shell
                    .program_location_cache_mut()
                    .set(name, path.clone());
            }
        } else {
            for name in &self.names {
                let _ = context.shell.program_location_cache_mut().unset(name);

                if name.contains('/') {
                    continue;
                }

                if context
                    .shell
                    .find_first_executable_in_path_using_cache(name)
                    .is_none()
                {
                    writeln!(stderr_output, "{name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        }

        if !output.is_empty() {
            if let Some(mut stdout) = context.stdout() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            }
        }

        if !stderr_output.is_empty() {
            if let Some(mut stderr) = context.stderr() {
                stderr.write_all(&stderr_output).await?;
                stderr.flush().await?;
            }
        }

        Ok(result)
    }
}
