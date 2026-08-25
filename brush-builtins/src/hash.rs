use std::{io::Write, path::PathBuf};

use brush_core::{ExecutionResult, builtins};

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

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_REMOVE,
            &['d'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Remove entries associated with the given names.",
        )
        .arg(
            ID_DISPLAY_AS_USABLE_INPUT,
            &['l'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Display paths in a format usable for input.",
        )
        .arg(
            ID_PATH_TO_USE,
            &['p'],
            &[],
            builtins::argmodel::ArgKind::Value,
            Some("PATH"),
            "The path to associate with the names.",
        )
        .arg(
            ID_REMOVE_ALL,
            &['r'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Remove all entries.",
        )
        .arg(
            ID_DISPLAY_PATHS,
            &['t'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Display the paths associated with the names.",
        )
        .positional_many(ID_NAMES, "NAMES")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            remove: matches.flag(ID_REMOVE),
            display_as_usable_input: matches.flag(ID_DISPLAY_AS_USABLE_INPUT),
            path_to_use: matches.value(ID_PATH_TO_USE).map(PathBuf::from),
            remove_all: matches.flag(ID_REMOVE_ALL),
            display_paths: matches.flag(ID_DISPLAY_PATHS),
            names: matches.values(ID_NAMES).to_vec(),
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
