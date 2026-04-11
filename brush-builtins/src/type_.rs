use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Parser;

use brush_core::sys::fs::PathExt;
use brush_core::{ExecutionResult, Shell, builtins, parser::ast};

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

enum ResolvedType<'a> {
    Alias(String),
    Keyword,
    Function(&'a ast::FunctionDefinition),
    Builtin,
    File { path: PathBuf, hashed: bool },
}

impl builtins::Command for TypeCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();
        let mut output = Vec::new();
        let mut stderr_output = Vec::new();

        for name in &self.names {
            let resolved_types = self.resolve_types(context.shell, name);

            if resolved_types.is_empty() {
                if !self.type_only && !self.force_path_search && !self.show_path_only {
                    writeln!(stderr_output, "type: {name} not found")?;
                }

                result = ExecutionResult::general_error();
                continue;
            }

            for resolved_type in resolved_types {
                if self.show_path_only && !matches!(resolved_type, ResolvedType::File { .. }) {
                    // Do nothing.
                } else if self.type_only {
                    match resolved_type {
                        ResolvedType::Alias(_) => {
                            writeln!(output, "alias")?;
                        }
                        ResolvedType::Keyword => {
                            writeln!(output, "keyword")?;
                        }
                        ResolvedType::Function(_) => {
                            writeln!(output, "function")?;
                        }
                        ResolvedType::Builtin => {
                            writeln!(output, "builtin")?;
                        }
                        ResolvedType::File { path, .. } => {
                            if self.show_path_only || self.force_path_search {
                                writeln!(output, "{}", path.to_string_lossy())?;
                            } else {
                                writeln!(output, "file")?;
                            }
                        }
                    }
                } else {
                    match resolved_type {
                        ResolvedType::Alias(target) => {
                            writeln!(output, "{name} is aliased to `{target}'")?;
                        }
                        ResolvedType::Keyword => {
                            writeln!(output, "{name} is a shell keyword")?;
                        }
                        ResolvedType::Function(def) => {
                            writeln!(output, "{name} is a function")?;
                            writeln!(output, "{def}")?;
                        }
                        ResolvedType::Builtin => {
                            writeln!(output, "{name} is a shell builtin")?;
                        }
                        ResolvedType::File { path, hashed } => {
                            if hashed && self.all_locations && !self.force_path_search {
                                // Do nothing.
                            } else if self.show_path_only || self.force_path_search {
                                writeln!(output, "{}", path.to_string_lossy())?;
                            } else if hashed {
                                writeln!(
                                    output,
                                    "{name} is hashed ({path})",
                                    name = name,
                                    path = path.to_string_lossy()
                                )?;
                            } else {
                                writeln!(
                                    output,
                                    "{name} is {path}",
                                    name = name,
                                    path = path.to_string_lossy()
                                )?;
                            }
                        }
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

impl TypeCommand {
    fn resolve_types<'a, SE: brush_core::ShellExtensions>(
        &self,
        shell: &'a Shell<SE>,
        name: &str,
    ) -> Vec<ResolvedType<'a>> {
        let mut types = vec![];

        if !self.force_path_search {
            // Check for aliases.
            if let Some(a) = shell.aliases().get(name) {
                types.push(ResolvedType::Alias(a.clone()));
                if !self.all_locations {
                    return types;
                }
            }

            // Check for keywords.
            if shell.is_keyword(name) {
                types.push(ResolvedType::Keyword);
                if !self.all_locations {
                    return types;
                }
            }

            // Check for functions.
            if !self.suppress_func_lookup {
                if let Some(registration) = shell.funcs().get(name) {
                    types.push(ResolvedType::Function(registration.definition()));
                    if !self.all_locations {
                        return types;
                    }
                }
            }

            // Check for builtins.
            if shell.builtins().get(name).is_some_and(|b| !b.disabled) {
                types.push(ResolvedType::Builtin);
                if !self.all_locations {
                    return types;
                }
            }
        }

        // Look in path.
        if name.contains(std::path::MAIN_SEPARATOR) {
            if shell.absolute_path(Path::new(name)).executable() {
                types.push(ResolvedType::File {
                    path: PathBuf::from(name),
                    hashed: false,
                });

                if !self.all_locations {
                    return types;
                }
            }
        } else {
            if let Some(path) = shell.program_location_cache().get(name) {
                types.push(ResolvedType::File { path, hashed: true });
                if !self.all_locations {
                    return types;
                }
            }

            for item in shell.find_executables_in_path(name) {
                types.push(ResolvedType::File {
                    path: item,
                    hashed: false,
                });

                if !self.all_locations {
                    return types;
                }
            }
        }

        types
    }
}
