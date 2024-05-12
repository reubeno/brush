use std::path::PathBuf;
use std::{io::Write, sync::Arc};

use clap::Parser;
use parser::ast;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    Shell,
};

#[derive(Parser)]
pub(crate) struct TypeCommand {
    #[arg(short = 'a')]
    all_locations: bool,

    #[arg(short = 'f')]
    suppress_func_lookup: bool,

    #[arg(short = 'P')]
    force_path_search: bool,

    #[arg(short = 'p')]
    show_path_only: bool,

    #[arg(short = 't')]
    type_only: bool,

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
impl BuiltinCommand for TypeCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut result = BuiltinExitCode::Success;

        for name in &self.names {
            let resolved_types = self.resolve_types(context.shell, name);

            if resolved_types.is_empty() {
                if !self.type_only && !self.force_path_search {
                    writeln!(context.stderr(), "type: {name} not found")?;
                }

                result = BuiltinExitCode::Custom(1);
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
            if is_keyword(shell, name) {
                types.push(ResolvedType::Keyword);
            }

            // Check for functions.
            if !self.suppress_func_lookup {
                if let Some(def) = shell.funcs.get(name) {
                    types.push(ResolvedType::Function(def.clone()));
                }
            }

            // Check for builtins.
            if crate::builtins::SPECIAL_BUILTINS.contains_key(name)
                || crate::builtins::BUILTINS.contains_key(name)
            {
                types.push(ResolvedType::Builtin);
            }
        }

        // Look in path.
        if name.contains('/') {
            // TODO: Handle this case.
        } else {
            for item in shell.find_executables_in_path(name) {
                types.push(ResolvedType::File(item));
            }
        }

        types
    }
}

fn is_keyword(shell: &Shell, name: &str) -> bool {
    match name {
        "!" => true,
        "{" => true,
        "}" => true,
        "case" => true,
        "do" => true,
        "done" => true,
        "elif" => true,
        "else" => true,
        "esac" => true,
        "fi" => true,
        "for" => true,
        "if" => true,
        "in" => true,
        "then" => true,
        "until" => true,
        "while" => true,
        // N.B. Some shells also treat the following as reserved.
        "[[" if !shell.options.sh_mode => true,
        "]]" if !shell.options.sh_mode => true,
        "function" if !shell.options.sh_mode => true,
        "select" if !shell.options.sh_mode => true,
        _ => false,
    }
}
