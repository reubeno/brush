use std::{io::Write, path::PathBuf};

use brush_core::{
    ExecutionResult,
    argmodel::{ArgSpec, PositionalSpec},
    builtins,
};

pub(crate) struct HashCommand {
    remove: bool,
    display_as_usable_input: bool,
    path_to_use: Option<PathBuf>,
    remove_all: bool,
    display_paths: bool,
    names: Vec<String>,
}

const ID_REMOVE: &str = "remove";
const ID_DISPLAY_AS_USABLE_INPUT: &str = "display_as_usable_input";
const ID_PATH_TO_USE: &str = "path_to_use";
const ID_REMOVE_ALL: &str = "remove_all";
const ID_DISPLAY_PATHS: &str = "display_paths";
const ID_NAMES: &str = "names";

impl builtins::SpecCommand for HashCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[
                ArgSpec::flag(
                    ID_REMOVE,
                    &['d'],
                    &[],
                    "Remove entries associated with the given names.",
                ),
                ArgSpec::flag(
                    ID_DISPLAY_AS_USABLE_INPUT,
                    &['l'],
                    &[],
                    "Display paths in a format usable for input.",
                ),
                ArgSpec::value(
                    ID_PATH_TO_USE,
                    &['p'],
                    &[],
                    "PATH",
                    "The path to associate with the names.",
                ),
                ArgSpec::flag(ID_REMOVE_ALL, &['r'], &[], "Remove all entries."),
                ArgSpec::flag(
                    ID_DISPLAY_PATHS,
                    &['t'],
                    &[],
                    "Display the paths associated with the names.",
                ),
            ],
            positionals: &[PositionalSpec::many(ID_NAMES, "NAMES")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            remove: values.flag(ID_REMOVE),
            display_as_usable_input: values.flag(ID_DISPLAY_AS_USABLE_INPUT),
            path_to_use: values.value(ID_PATH_TO_USE).map(PathBuf::from),
            remove_all: values.flag(ID_REMOVE_ALL),
            display_paths: values.flag(ID_DISPLAY_PATHS),
            names: values.positional_values(ID_NAMES).to_vec(),
        })
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
