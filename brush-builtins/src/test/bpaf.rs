//! `test` builtin: `TestCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

fn execute_test(
    shell: &mut brush_core::Shell<impl brush_core::ShellExtensions>,
    params: &brush_core::ExecutionParameters,
    args: &[String],
) -> Result<bool, brush_core::Error> {
    let test_command =
        brush_parser::test_command::parse(args).map_err(brush_core::ErrorKind::TestCommandParseError)?;
    brush_core::tests::eval_expr(&test_command, shell, params)
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn)]
mod double_dash_tests {
    use super::*;
    use brush_core::builtins::Command as _;

    #[test]
    fn captures_lone_double_dash() -> anyhow::Result<()> {
        let cmd = TestCommand::new(["test", "--"].iter().map(|s| s.to_string()))?;
        assert_eq!(cmd.args, ["--"]);
        Ok(())
    }
}

/// Evaluate test expression.
pub(crate) struct TestCommand {
    /// The arguments, interpreted as a test expression.
    pub(super) args: Vec<String>,
}

impl crate::args::BpafArgs for TestCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Arguments are captured verbatim in [`Self::new`] because test
        // expressions are interpreted entirely by `execute`; the parser exists
        // only for help rendering.
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(TestCommand { args })
    }
fn about() -> &'static str {
        "Evaluate test expression."
    }
fn synopsis() -> &'static str {
        "[EXPRESSION]"
    }
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();

        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        Ok(Self { args })
    
    }
}

impl FromArgs for TestCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::BpafArgs::from_words(words)
    }
}

impl builtins::Command for TestCommand {
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
