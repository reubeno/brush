use brush_core::{
    ExecutionResult,
    argmodel::{ArgSpec, PositionalSpec},
    builtins,
};

/// Push a path onto the current directory stack.
pub(crate) struct PushdCommand {
    no_directory_change: bool,
    dir: String,
}

const ID_NO_DIRECTORY_CHANGE: &str = "no_directory_change";
const ID_DIR: &str = "dir";

impl builtins::SpecCommand for PushdCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        // TODO(pushd): implement +N and -N
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[ArgSpec::flag(
                ID_NO_DIRECTORY_CHANGE,
                &['n'],
                &[],
                "Push the path without changing the current working directory.",
            )],
            positionals: &[PositionalSpec::one(ID_DIR, "DIR")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let dir =
            values
                .value_of_positional(ID_DIR)
                .ok_or_else(|| builtins::BuiltinArgParseError {
                    message: "missing required argument: DIR".to_string(),
                    help_request: false,
                })?;

        Ok(Self {
            no_directory_change: values.flag(ID_NO_DIRECTORY_CHANGE),
            dir: dir.to_owned(),
        })
    }

    fn about() -> &'static str {
        "Push a path onto the current directory stack."
    }

    fn synopsis() -> &'static str {
        "[-n] [DIR]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.no_directory_change {
            context
                .shell
                .directory_stack_mut()
                .push(std::path::PathBuf::from(&self.dir));
        } else {
            let prev_working_dir = context.shell.working_dir().to_path_buf();

            let dir = std::path::Path::new(&self.dir);
            context.shell.set_working_dir(dir)?;

            context.shell.directory_stack_mut().push(prev_working_dir);
        }

        // Display dirs.
        let dirs_cmd = crate::dirs::DirsCommand::default();
        dirs_cmd.execute(context).await?;

        Ok(ExecutionResult::success())
    }
}
