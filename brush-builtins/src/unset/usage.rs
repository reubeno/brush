//! `unset` builtin: `UnsetCommand` instrumented for usage-rs.

#![cfg(feature = "parser-usage")]

use std::ffi::OsStr;

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Unset a variable.
pub(crate) struct UnsetCommand {
    pub(super) name_interpretation: UnsetNameInterpretation,
    pub(super) names: Vec<String>,
}

/// How each provided name should be interpreted.
#[derive(Default)]
pub(crate) struct UnsetNameInterpretation {
    /// Treat each name as a shell function.
    pub(super) shell_functions: bool,

    /// Treat each name as a shell variable.
    pub(super) shell_variables: bool,

    /// Treat each name as a name reference.
    pub(super) name_references: bool,
}


impl crate::args::UsageArgs for UnsetCommand {
    fn parse_argv<'v>(
        _argv: &[&'v OsStr],
    ) -> Result<Self, usage::argv::Error<'static, 'v>> {
        // Parsing is fully handled by [`Self::from_words`].
        unreachable!("unset parses via from_words")
    }

    #[doc(hidden)]
    fn usage_spec() -> &'static usage::spec::Spec<'static> {
        unreachable!("unset parses via from_words")
    }

    fn about() -> &'static str {
        "Unset values and attributes of shell variables and functions."
    }

    fn synopsis() -> &'static str {
        "[-fnv] [NAME]..."
    }

    // N.B. Options are interpreted manually to mirror bash's grouped `-fnv`
    // handling without requiring a spec round-trip.
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let mut args = words.to_vec();

        // N.B. The first word is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        let mut interpretation = UnsetNameInterpretation::default();
        let mut names = Vec::new();
        let mut terminated = false;

        for arg in args {
            if !terminated && arg == "--" {
                terminated = true;
                continue;
            }

            if !terminated {
                if let Some(group) = arg
                    .strip_prefix('-')
                    .filter(|g| !g.is_empty() && g.chars().all(|c| matches!(c, 'f' | 'n' | 'v')))
                {
                    for c in group.chars() {
                        match c {
                            'f' => interpretation.shell_functions = true,
                            'n' => interpretation.name_references = true,
                            'v' => interpretation.shell_variables = true,
                            _ => {}
                        }
                    }
                    continue;
                }

                if arg.starts_with('-') && arg != "-" {
                    return Err(ArgsError::new(
                        "unset: invalid option\nUsage: unset [-fnv] [NAME]...",
                    ));
                }
            }

            names.push(arg);
        }

        Ok(Self {
            name_interpretation: interpretation,
            names,
        })
    }
}

impl FromArgs for UnsetCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::UsageArgs::from_words(words)
    }
}

impl builtins::Command for UnsetCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        use std::ffi::OsStr;
        let _ = (name, content_type, options);
        unreachable!("unset renders help via about/synopsis")
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
