use brush_core::{ExecutionResult, argmodel::ArgSpec, builtins};

/// Pop a path from the current directory stack.
pub(crate) struct PopdCommand {
    no_directory_change: bool,
}

const ID_NO_DIRECTORY_CHANGE: &str = "no_directory_change";

impl builtins::SpecCommand for PopdCommand {
    type Error = crate::dirs::DirError;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        // TODO(popd): implement +N and -N
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[ArgSpec::flag(
                ID_NO_DIRECTORY_CHANGE,
                &['n'],
                &[],
                "Pop the path without changing the current working directory.",
            )],
            positionals: &[],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            no_directory_change: values.flag(ID_NO_DIRECTORY_CHANGE),
        })
    }

    fn about() -> &'static str {
        "Pop a path from the current directory stack."
    }

    fn synopsis() -> &'static str {
        "[-n]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if let Some(popped) = context.shell.directory_stack_mut().pop() {
            if !self.no_directory_change {
                context.shell.set_working_dir(&popped)?;
            }

            // Display dirs.
            let dirs_cmd = crate::dirs::DirsCommand::default();
            dirs_cmd.execute(context).await?;

            Ok(ExecutionResult::success())
        } else {
            Err(crate::dirs::DirError::DirStackEmpty)
        }
    }
}
