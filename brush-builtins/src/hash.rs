use bpaf::Bpaf;
use std::{io::Write, path::PathBuf};

use brush_core::{ExecutionResult, builtins};

#[derive(Bpaf)]
pub(crate) struct HashCommand {
    /// Remove entries associated with the given names.
    #[bpaf(short('d'))]
    remove: bool,

    /// Display paths in a format usable for input.
    #[bpaf(short('l'))]
    display_as_usable_input: bool,

    /// The path to associate with the names.
    #[bpaf(short('p'), argument("PATH"))]
    path_to_use: Option<PathBuf>,

    /// Remove all entries.
    #[bpaf(short('r'))]
    remove_all: bool,

    /// Display the paths associated with the names.
    #[bpaf(short('t'))]
    display_paths: bool,

    /// Names to process.
    #[bpaf(positional("NAMES"))]
    names: Vec<String>,
}

impl builtins::Command for HashCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        hash_command()
    }

    fn about() -> &'static str {
        "Remember or display program locations."
    }

    fn synopsis() -> &'static str {
        "[-dlrt] [-p PATH] [NAMES]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionResult::success();

        if self.remove_all {
            context.shell.program_location_cache_mut().reset();
        } else if self.remove {
            for name in &self.names {
                if !context.shell.program_location_cache_mut().unset(name) {
                    writeln!(context.stderr(), "{name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        } else if self.display_paths {
            for name in &self.names {
                if let Some(path) = context.shell.program_location_cache().get(name) {
                    if self.display_as_usable_input {
                        writeln!(
                            context.stdout(),
                            "builtin hash -p {} {name}",
                            path.to_string_lossy()
                        )?;
                    } else {
                        let mut prefix = String::new();

                        if self.names.len() > 1 {
                            prefix.push_str(name.as_str());
                            prefix.push('\t');
                        }

                        writeln!(
                            context.stdout(),
                            "{prefix}{}",
                            path.to_string_lossy().as_ref()
                        )?;
                    }
                } else {
                    writeln!(context.stderr(), "{name}: not found")?;
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
                    writeln!(context.stderr(), "{name}: not found")?;
                    result = ExecutionResult::general_error();
                }
            }
        }

        Ok(result)
    }
}
