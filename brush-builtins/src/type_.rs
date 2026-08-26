//! The `type_` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(TypeCommand);

use brush_core::sys::{self, fs::PathExt};
use brush_core::{ExecutionResult, Shell, parser::ast};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) enum ResolvedType<'a> {
    Alias(String),
    Keyword,
    Function(&'a ast::FunctionDefinition),
    Builtin,
    File { path: PathBuf, hashed: bool },
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
        if sys::fs::contains_path_separator(name) {
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

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &TypeCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut result = ExecutionResult::success();

    for name in &command.names {
        let resolved_types = command.resolve_types(context.shell, name);

        if resolved_types.is_empty() {
            if !command.type_only && !command.force_path_search && !command.show_path_only {
                writeln!(context.stderr(), "type: {name} not found")?;
            }

            result = ExecutionResult::general_error();
            continue;
        }

        for resolved_type in resolved_types {
            if command.show_path_only && !matches!(resolved_type, ResolvedType::File { .. }) {
                // Do nothing.
            } else if command.type_only {
                match resolved_type {
                    ResolvedType::Alias(_) => {
                        writeln!(context.stdout(), "alias")?;
                    }
                    ResolvedType::Keyword => {
                        writeln!(context.stdout(), "keyword")?;
                    }
                    ResolvedType::Function(_) => {
                        writeln!(context.stdout(), "function")?;
                    }
                    ResolvedType::Builtin => {
                        writeln!(context.stdout(), "builtin")?;
                    }
                    ResolvedType::File { path, .. } => {
                        if command.show_path_only || command.force_path_search {
                            writeln!(context.stdout(), "{}", path.to_string_lossy())?;
                        } else {
                            writeln!(context.stdout(), "file")?;
                        }
                    }
                }
            } else {
                match resolved_type {
                    ResolvedType::Alias(target) => {
                        writeln!(context.stdout(), "{name} is aliased to `{target}'")?;
                    }
                    ResolvedType::Keyword => {
                        writeln!(context.stdout(), "{name} is a shell keyword")?;
                    }
                    ResolvedType::Function(def) => {
                        writeln!(context.stdout(), "{name} is a function")?;
                        writeln!(context.stdout(), "{def}")?;
                    }
                    ResolvedType::Builtin => {
                        writeln!(context.stdout(), "{name} is a shell builtin")?;
                    }
                    ResolvedType::File { path, hashed } => {
                        if hashed && command.all_locations && !command.force_path_search {
                            // Do nothing. When we're displaying all locations, then
                            // we don't show hashed paths.
                        } else if command.show_path_only || command.force_path_search {
                            writeln!(context.stdout(), "{}", path.to_string_lossy())?;
                        } else if hashed {
                            writeln!(
                                context.stdout(),
                                "{name} is hashed ({path})",
                                name = name,
                                path = path.to_string_lossy()
                            )?;
                        } else {
                            writeln!(
                                context.stdout(),
                                "{name} is {path}",
                                name = name,
                                path = path.to_string_lossy()
                            )?;
                        }
                    }
                }
            }

            // If we only want the first, then break after the first.
            if !command.all_locations {
                break;
            }
        }
    }

    Ok(result)
}
