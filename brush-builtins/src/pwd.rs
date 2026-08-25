use brush_core::{ExecutionResult, builtins};
use std::{borrow::Cow, io::Write, path::Path};

/// Display the current working directory.
#[derive(Clone)]
pub(crate) struct PwdCommand {
    /// Whether an explicit physical/logical mode was requested; `Some(true)`
    /// means physical (`-P`) and `Some(false)` means logical (`-L`). When both
    /// are provided, the last one on the command line wins.
    mode: Option<bool>,
}

impl builtins::SpecCommand for PwdCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        &SPEC
    }

    fn from_matches(
        _values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        // N.B. Parsing is fully handled by the overridden `new`.
        unreachable!("pwd parses via overridden new()")
    }
    fn about() -> &'static str {
        "Display the current working directory."
    }

    fn synopsis() -> &'static str {
        "[-LP]"
    }

    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        // N.B. Options are interpreted manually because their combined forms
        // depend on ordering (`pwd -L -P` vs `pwd -P -L`).
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        let mut mode: Option<bool> = None;
        let mut terminated = false;

        for arg in args {
            if !terminated && arg == "--" {
                terminated = true;
                continue;
            }

            if !terminated {
                if let Some(group) = arg
                    .strip_prefix('-')
                    .filter(|g| !g.is_empty() && g.chars().all(|c| c == 'L' || c == 'P'))
                {
                    if let Some(c) = group.chars().last() {
                        mode = Some(c == 'P');
                    }
                    continue;
                }

                if arg.starts_with('-') && arg != "-" {
                    return Err(builtins::BuiltinArgParseError {
                        message: String::from("pwd: invalid option\nUsage: pwd [-LP]"),
                        help_request: false,
                    });
                }
            }

            return Err(builtins::BuiltinArgParseError {
                message: String::from("pwd: too many arguments"),
                help_request: false,
            });
        }

        Ok(Self { mode })
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut cwd: Cow<'_, Path> = context.shell.working_dir().into();

        let should_canonicalize = self.mode == Some(true)
            || context
                .shell
                .options()
                .do_not_resolve_symlinks_when_changing_dir;

        if should_canonicalize {
            cwd = cwd.canonicalize()?.into();
        }

        writeln!(context.stdout(), "{}", cwd.to_string_lossy())?;

        Ok(ExecutionResult::success())
    }
}

static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec::EMPTY;

#[cfg(test)]
mod tests {
    use super::*;
    use brush_core::builtins::SpecCommand as _;

    #[test]
    fn parse_modes() {
        assert_eq!(
            PwdCommand::new(std::iter::once("pwd".to_string()))
                .unwrap()
                .mode,
            None
        );
        assert_eq!(
            PwdCommand::new(["pwd", "-L"].iter().map(|s| s.to_string()))
                .unwrap()
                .mode,
            Some(false)
        );
        assert_eq!(
            PwdCommand::new(["pwd", "-P"].iter().map(|s| s.to_string()))
                .unwrap()
                .mode,
            Some(true)
        );

        // Last one wins.
        assert_eq!(
            PwdCommand::new(["pwd", "-L", "-P"].iter().map(|s| s.to_string()))
                .unwrap()
                .mode,
            Some(true)
        );
        assert_eq!(
            PwdCommand::new(["pwd", "-P", "-L"].iter().map(|s| s.to_string()))
                .unwrap()
                .mode,
            Some(false)
        );
        assert_eq!(
            PwdCommand::new(["pwd", "-LP"].iter().map(|s| s.to_string()))
                .unwrap()
                .mode,
            Some(true)
        );
        assert_eq!(
            PwdCommand::new(["pwd", "-PL"].iter().map(|s| s.to_string()))
                .unwrap()
                .mode,
            Some(false)
        );
    }
}
