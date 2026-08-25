//! clap-backed arguments for `echo`.

use brush_core::args::ArgsError;
use brush_core::builtins;
use clap::Parser;

/// Echo text to standard output.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    #[arg(short = 'n')]
    pub(super) no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[arg(short = 'e')]
    pub(super) interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    // N.B. Parsed for parity with bash's `-E`; not yet consulted by execute.
    #[expect(dead_code)]
    #[arg(short = 'E')]
    pub(super) no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(super) args: Vec<String>,
}

impl builtins::Command for EchoCommand {
    type Error = brush_core::Error;

    // Override the default [`builtins::Command::new`] function to handle clap's limitation
    // related to `--`. See [`builtins::parse_known`] for more information.
    // TODO: we can safely remove this after the issue is resolved
    fn new<I>(args: I) -> Result<Self, ArgsError>
    where
        I: IntoIterator<Item = String>,
    {
        let (mut this, rest_args) = builtins::try_parse_known::<Self>(args)
            .map_err(|err| ArgsError::from_clap_error(&err))?;
        if let Some(args) = rest_args {
            this.args.extend(args);
        }

        Ok(this)
    }

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        // N.B. Transitional: rendered from this struct's engine metadata;
        // replaced by brush's own help model when it exists.
        builtins::clap_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
