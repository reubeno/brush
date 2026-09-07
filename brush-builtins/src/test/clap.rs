//! `test` builtin: `TestCommand` instrumented for clap.

#![cfg(feature = "parser-clap")]

use clap::Parser;
use brush_core::builtins;

/// Evaluate test expression.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct TestCommand {
    #[clap(allow_hyphen_values = true)]
    pub(super) args: Vec<String>,
}

impl builtins::Command for TestCommand {
    type Error = brush_core::Error;

    /// Override the default [`builtins::Command::new`] function to handle clap's limitation related
    /// to `--`. See [`builtins::parse_known`] for more information
    /// TODO(test): we can safely remove this after the issue is resolved
    fn new<I>(args: I) -> Result<Self, brush_core::args::ArgsError>
    where
        I: IntoIterator<Item = String>,
    {
        let (mut this, rest_args) = brush_core::builtins::try_parse_known::<Self>(args)
            .map_err(|err| brush_core::args::ArgsError::from_clap_error(&err))?;
        if let Some(args) = rest_args {
            this.args.extend(args);
        }
        Ok(this)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: help still rendered from clap-derived metadata.
        builtins::clap_content::<Self>(name, &content_type, options)
    }
}
