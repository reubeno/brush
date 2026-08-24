use std::io::Write;

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionParameters, ExecutionResult, Shell, builtins, tests,
};

/// Evaluate test expression.
pub(crate) struct TestCommand {
    /// The arguments, interpreted as a test expression.
    args: Vec<String>,
}

impl builtins::Command for TestCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Arguments are captured verbatim in [`Self::new`] because test
        // expressions are interpreted entirely by `execute`; the parser exists
        // only for help rendering.
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(TestCommand { args })
    }

    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        Ok(Self { args })
    }

    fn about() -> &'static str {
        "Evaluate test expression."
    }

    fn synopsis() -> &'static str {
        "[EXPRESSION]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut args = self.args.as_slice();

        if context.command_name == "[" {
            match args.last() {
                Some(s) if s == "]" => (),
                None | Some(_) => {
                    writeln!(context.stderr(), "[: missing ']'")?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }
            }

            args = &args[0..args.len() - 1];
        }

        // N.B. A leading `--` operand ends option processing and is removed,
        // except when it is the only operand, in which case it is treated as a
        // non-empty string argument; both behaviors match bash.
        if args.first().map(String::as_str) == Some("--") {
            args = if args.len() == 1 {
                return Ok(ExecutionResult::success());
            } else {
                &args[1..]
            };
        }

        if execute_test(context.shell, &context.params, args)? {
            Ok(ExecutionResult::success())
        } else {
            Ok(ExecutionResult::general_error())
        }
    }
}

fn execute_test(
    shell: &mut Shell<impl brush_core::ShellExtensions>,
    params: &ExecutionParameters,
    args: &[String],
) -> Result<bool, brush_core::Error> {
    let test_command =
        brush_parser::test_command::parse(args).map_err(ErrorKind::TestCommandParseError)?;
    tests::eval_expr(&test_command, shell, params)
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
