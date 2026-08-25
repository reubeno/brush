use std::io::Write;

use brush_core::{ExecutionResult, builtins, escape};

/// Echo text to standard output.
///
/// N.B. The struct carries the argument instrumentation for the selected
/// parsing engine (see the `parser-*` features); with `parser-clap`, the
/// blanket [`brush_core::args::FromArgs`] implementation covers binding and
/// only the `--` quirk needs an override.
#[cfg_attr(
    feature = "parser-clap",
    derive(clap::Parser),
    clap(disable_help_flag = true, disable_version_flag = true)
)]
// N.B. Additional engines attach their derives/attributes here as they land.
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    #[cfg_attr(feature = "parser-clap", arg(short = 'n'))]
    no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[cfg_attr(feature = "parser-clap", arg(short = 'e'))]
    interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    #[cfg_attr(feature = "parser-clap", arg(short = 'E'))]
    no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[cfg_attr(
        feature = "parser-clap",
        arg(trailing_var_arg = true, allow_hyphen_values = true)
    )]
    args: Vec<String>,
}

impl builtins::Command for EchoCommand {
    type Error = brush_core::Error;

    #[cfg(feature = "parser-clap")]
    fn new<I>(args: I) -> Result<Self, brush_core::args::ArgsError>
    where
        I: IntoIterator<Item = String>,
    {
        // Override the default [`builtins::Command::new`] function to handle clap's limitation
        // related to `--`. See [`builtins::parse_known`] for more information.
        // TODO: we can safely remove this after the issue is resolved
        let (mut this, rest_args) = brush_core::builtins::try_parse_known::<Self>(args)
            .map_err(|err| brush_core::args::ArgsError::from_clap_error(&err))?;
        if let Some(args) = rest_args {
            this.args.extend(args);
        }

        Ok(this)
    }

    #[cfg(not(feature = "parser-clap"))]
    fn get_content(
        _name: &str,
        _content_type: builtins::ContentType,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        compile_error!("help rendering is not yet available for this argument engine")
    }

    #[cfg(feature = "parser-clap")]
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
        let mut trailing_newline = !self.no_trailing_newline;
        let mut s;
        if self.interpret_backslash_escapes {
            s = String::new();
            for (i, arg) in self.args.iter().enumerate() {
                if i > 0 {
                    s.push(' ');
                }

                let (expanded_arg, keep_going) = escape::expand_backslash_escapes(
                    arg.as_str(),
                    escape::EscapeExpansionMode::EchoBuiltin,
                )?;
                s.push_str(&String::from_utf8_lossy(expanded_arg.as_slice()));

                if !keep_going {
                    trailing_newline = false;
                    break;
                }
            }
        } else {
            s = self.args.join(" ");
        }

        if trailing_newline {
            s.push('\n');
        }

        write!(context.stdout(), "{s}")?;
        context.stdout().flush()?;

        Ok(ExecutionResult::success())
    }
}
