use std::io::Write;
use std::path::{Path, PathBuf};

use brush_core::sys::{self, fs::PathExt};
use brush_core::{ExecutionResult, Shell, builtins, parser::ast};

/// Inspect the type of a named shell item.
pub(crate) struct TypeCommand {
    all_locations: bool,
    suppress_func_lookup: bool,
    force_path_search: bool,
    show_path_only: bool,
    type_only: bool,
    names: Vec<String>,
}

const ID_ALL_LOCATIONS: &str = "all_locations";
const ID_SUPPRESS_FUNC_LOOKUP: &str = "suppress_func_lookup";
const ID_FORCE_PATH_SEARCH: &str = "force_path_search";
const ID_SHOW_PATH_ONLY: &str = "show_path_only";
const ID_TYPE_ONLY: &str = "type_only";
const ID_NAMES: &str = "names";

enum ResolvedType<'a> {
    Alias(String),
    Keyword,
    Function(&'a ast::FunctionDefinition),
    Builtin,
    File { path: PathBuf, hashed: bool },
}

impl builtins::SpecCommand for TypeCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_ALL_LOCATIONS,
            &['a'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Display all locations of the specified name, not just the first.",
        )
        .arg(
            ID_SUPPRESS_FUNC_LOOKUP,
            &['f'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Don't consider functions when resolving the name.",
        )
        .arg(
            ID_FORCE_PATH_SEARCH,
            &['P'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Force searching by file path, even if the name is an alias, built-in command, or shell function.",
        )
        .arg(
            ID_SHOW_PATH_ONLY,
            &['p'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Show file path only.",
        )
        .arg(
            ID_TYPE_ONLY,
            &['t'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Only display the type of the specified name.",
        )
        .positional_many(ID_NAMES, "NAMES")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            all_locations: matches.flag(ID_ALL_LOCATIONS),
            suppress_func_lookup: matches.flag(ID_SUPPRESS_FUNC_LOOKUP),
            force_path_search: matches.flag(ID_FORCE_PATH_SEARCH),
            show_path_only: matches.flag(ID_SHOW_PATH_ONLY),
            type_only: matches.flag(ID_TYPE_ONLY),
            names: matches.values(ID_NAMES).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Inspect the type of a named shell item."
    }

    fn synopsis() -> &'static str {
        "[-aPptf] [NAMES]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();

        for name in &self.names {
            let resolved_types = self.resolve_types(context.shell, name);

            if resolved_types.is_empty() {
                if !self.type_only && !self.force_path_search && !self.show_path_only {
                    writeln!(context.stderr(), "type: {name} not found")?;
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
                            if hashed && self.all_locations && !self.force_path_search {
                                // Do nothing. When we're displaying all locations, then
                                // we don't show hashed paths.
                            } else if self.show_path_only || self.force_path_search {
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
                if !self.all_locations {
                    break;
                }
            }
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
