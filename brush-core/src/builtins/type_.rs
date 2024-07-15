use std::path::PathBuf;
use std::{io::Write, sync::Arc};

use brush_parser::ast;
use clap::Parser;

use crate::keywords;
use crate::sys::fs::PathExt;
use crate::{builtins, commands, Shell};

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

enum ResolvedType {
    Alias(String),
    Keyword,
    Function(Arc<ast::FunctionDefinition>),
    Builtin,
    File(PathBuf),
}

#[async_trait::async_trait]
impl builtins::Command for TypeCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let mut result = builtins::ExitCode::Success;

        for name in &self.names {
            let resolved_types = self.resolve_types(context.shell, name);

            if resolved_types.is_empty() {
                if !self.type_only && !self.force_path_search {
                    writeln!(context.stderr(), "type: {name} not found")?;
                }

                result = builtins::ExitCode::Custom(1);
                continue;
            }

            for resolved_type in resolved_types {
                if self.show_path_only && !matches!(resolved_type, ResolvedType::File(_)) {
                    // Do nothing.
                } else if self.type_only {
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
                        ResolvedType::File(path) => {
                            if self.show_path_only || self.force_path_search {
                                writeln!(context.stdout(), "{}", path.to_string_lossy())?;
                            } else {
                                writeln!(context.stdout(), "file")?;
                            }
                        }
                    }
                } else {
                    match resolved_type {
                        ResolvedType::Alias(target) => {
                            writeln!(context.stdout(), "{name} is aliased to '{target}'")?;
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
                        ResolvedType::File(path) => {
                            if self.show_path_only || self.force_path_search {
                                writeln!(context.stdout(), "{}", path.to_string_lossy())?;
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
                if !self.all_locations {
                    break;
                }
            }
        }

        Ok(result)
    }
}

impl TypeCommand {
    fn resolve_types(&self, shell: &Shell, name: &str) -> Vec<ResolvedType> {
        let mut types = vec![];

        if !self.force_path_search {
            // Check for aliases.
            if let Some(a) = shell.aliases.get(name) {
                types.push(ResolvedType::Alias(a.clone()));
            }

            // Check for keywords.
            if keywords::is_keyword(shell, name) {
                types.push(ResolvedType::Keyword);
            }

            // Check for functions.
            if !self.suppress_func_lookup {
                if let Some(registration) = shell.funcs.get(name) {
                    types.push(ResolvedType::Function(registration.definition.clone()));
                }
            }

            // Check for builtins.
            if shell.builtins.get(name).is_some_and(|b| !b.disabled) {
                types.push(ResolvedType::Builtin);
            }
        }

        // Look in path.
        if name.contains(std::path::MAIN_SEPARATOR) {
            if std::path::Path::new(name).executable() {
                types.push(ResolvedType::File(PathBuf::from(name)));
            }
        } else {
            for item in shell.find_executables_in_path(name) {
                types.push(ResolvedType::File(item));
            }
        }

        types
    }
}
