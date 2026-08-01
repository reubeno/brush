use std::io::Write;

use clap::Parser;

use brush_core::{ExecutionResult, builtins};

use crate::lookup::{self, Resolved};

/// Inspect the type of a named shell item.
#[derive(Parser)]
pub(crate) struct TypeCommand {
    /// Display all locations of the specified name, not just the first.
    #[arg(short = 'a')]
    all_locations: bool,

    /// Don't consider functions when resolving the name.
    #[arg(short = 'f')]
    suppress_func_lookup: bool,

    /// Force searching by file path, even if the name is an alias, built-in
    /// command, or shell function.
    #[arg(short = 'P')]
    force_path_search: bool,

    /// Show file path only.
    #[arg(short = 'p')]
    show_path_only: bool,

    /// Only display the type of the specified name.
    #[arg(short = 't')]
    type_only: bool,

    /// Names to search for.
    names: Vec<String>,
}

impl builtins::Command for TypeCommand {
    type State = ();
    type SharedState = ();
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        let options = lookup::Options {
            force_path_search: self.force_path_search,
            suppress_func_lookup: self.suppress_func_lookup,
            all_locations: self.all_locations,
            path_dirs: None,
        };
        let mut output = Vec::new();
        let mut stderr_output = Vec::new();

        for name in &self.names {
            let resolved_types = lookup::resolve(context.shell, name, &options);

            if resolved_types.is_empty() {
                if !self.type_only && !self.force_path_search && !self.show_path_only {
                    writeln!(stderr_output, "type: {name}: not found")?;
                }

                result = ExecutionResult::general_error();
                continue;
            }

            for resolved_type in resolved_types {
                if self.show_path_only && !matches!(resolved_type, Resolved::File { .. }) {
                    // Do nothing.
                } else if self.type_only {
                    match &resolved_type {
                        Resolved::Alias(_) => {
                            writeln!(output, "alias")?;
                        }
                        Resolved::Keyword => {
                            writeln!(output, "keyword")?;
                        }
                        Resolved::Function(_) => {
                            writeln!(output, "function")?;
                        }
                        Resolved::Builtin => {
                            writeln!(output, "builtin")?;
                        }
                        Resolved::File { path, .. } => {
                            if self.show_path_only || self.force_path_search {
                                writeln!(output, "{}", path.to_string_lossy())?;
                            } else {
                                writeln!(output, "file")?;
                            }
                        }
                    }
                } else {
                    match &resolved_type {
                        // When we're displaying all locations, we don't show hashed paths.
                        Resolved::File { hashed: true, .. }
                            if self.all_locations && !self.force_path_search => {}
                        Resolved::File { path, .. }
                            if self.show_path_only || self.force_path_search =>
                        {
                            writeln!(output, "{}", path.to_string_lossy())?;
                        }
                        _ => lookup::describe(&mut output, name, &resolved_type)?,
                    }
                }

                if !self.all_locations {
                    break;
                }
            }
        }

        // Write output async
        if !output.is_empty() {
            if let Some(mut stdout) = context.stdout_async() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            } else {
                context.stdout().write_all(&output)?;
                context.stdout().flush()?;
            }
        }

        // Write stderr
        if !stderr_output.is_empty() {
            context.stderr().write_all(&stderr_output)?;
            context.stderr().flush()?;
        }

        Ok(result)
    }
}
