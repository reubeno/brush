use brush_core::{ExecutionResult, builtins};

/// Push a path onto the current directory stack.
pub(crate) struct PushdCommand {
    no_directory_change: bool,
    dir: String,
}

const ID_NO_DIRECTORY_CHANGE: &str = "no_directory_change";
const ID_DIR: &str = "dir";

impl builtins::SpecCommand for PushdCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_NO_DIRECTORY_CHANGE,
            &['n'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Push the path without changing the current working directory.",
        )
        .positional(ID_DIR, "DIR")
        // TODO(pushd): implement +N and -N
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let dir = matches
            .value(ID_DIR)
            .ok_or_else(|| builtins::BuiltinArgParseError {
                message: "missing required argument: DIR".to_string(),
                help_request: false,
            })?;

        Ok(Self {
            no_directory_change: matches.flag(ID_NO_DIRECTORY_CHANGE),
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
