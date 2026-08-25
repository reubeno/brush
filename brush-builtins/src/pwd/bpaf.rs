//! `pwd` builtin: `PwdCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Display the current working directory.
pub(crate) struct PwdCommand {
    /// Print the physical directory without any symlinks.
    pub(super) physical: bool,

    /// Print $PWD if it names the current working directory.
    pub(super) allow_symlinks: bool,
}

impl crate::args::BpafArgs for PwdCommand {
    fn parser() -> impl bpaf::Parser<Self> + 'static {
        let physical = bpaf::short('P')
            .help("Print the physical directory without any symlinks.")
            .switch();
        let allow_symlinks = bpaf::short('L')
            .help("Print $PWD if it names the current working directory.")
            .switch();

        bpaf::construct!(PwdCommand { physical, allow_symlinks })
    }

    fn about() -> &'static str {
        "Display the current working directory."
    }

    fn synopsis() -> &'static str {
        "[-LP]"
    }

    // N.B. Options are interpreted manually because their combined forms
    // depend on ordering (`pwd -L -P` vs `pwd -P -L`).
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let mut args = words.to_vec();

        // N.B. The first word is the command name itself.
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
                    return Err(ArgsError::new("pwd: invalid option\nUsage: pwd [-LP]"));
                }
            }

            return Err(ArgsError::new("pwd: too many arguments"));
        }

        Ok(Self {
            physical: mode == Some(true),
            allow_symlinks: mode == Some(false),
        })
    }
}

impl FromArgs for PwdCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for PwdCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
