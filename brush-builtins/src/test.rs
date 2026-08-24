use std::io::Write;

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionParameters, ExecutionResult, Shell, builtins, tests,
};

/// Evaluate test expression.
#[derive(usage::Cli)]
#[usage(
    bin = "test",
    unknown_flags = "value",
    args_override_self = false,
    disable_help_flag,
    disable_version_flag
)]
pub(crate) struct TestCommand {
    #[usage(arg, double_dash = "automatic")]
    args: Vec<String>,
}

impl builtins::Command for TestCommand {
    type Error = brush_core::Error;

    /// Override the default [`builtins::Command::new`] function to handle usage's limitation
    /// related to `--`. See [`builtins::parse_known`] for more information
    /// TODO(test): we can safely remove this after the issue is resolved
    fn new<I>(args: I) -> Result<Self, brush_core::builtins::ParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let (mut this, rest_args) = brush_core::builtins::try_parse_known::<Self>(args)?;
        if let Some(args) = rest_args {
            this.args.extend(args);
        }
        Ok(this)
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

        if execute_test(context.shell, &context.params, args)? {
            Ok(ExecutionResult::success())
        } else {
            Ok(ExecutionResult::general_error())
        }
    }
}

brush_core::impl_usage_parse!(TestCommand);

fn execute_test(
    shell: &mut Shell<impl brush_core::ShellExtensions>,
    params: &ExecutionParameters,
    args: &[String],
) -> Result<bool, brush_core::Error> {
    let test_command =
        brush_parser::test_command::parse(args).map_err(ErrorKind::TestCommandParseError)?;
    tests::eval_expr(&test_command, shell, params)
}
