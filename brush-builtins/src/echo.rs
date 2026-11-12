use brush_core::{ExecutionResult, builtins, escape};

/// Echo text to standard output.
#[derive(winnow_args::Args)]
#[arg(
    disable_help_flag,
    disable_version_flag,
    disable_help_subcommand,
    unknown_flags = "value"
)]
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    #[arg(short = 'n')]
    no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[arg(short = 'e', overrides = "-E")]
    interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    #[arg(short = 'E', overrides = "-e")]
    no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[arg(positional, double_dash = "preserve", stop_flags)]
    args: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(EchoCommand, keep_double_dash = args);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for EchoCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
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

        // Avoid async adapter overhead for non-stream outputs; keep async writes
        // for Stream (e.g. in-process channel pipes) which cooperate/yield.
        if let Some(out) = context.params.try_stdout(context.shell) {
            if matches!(out, brush_core::openfiles::OpenFile::Stream(_)) {
                out.write_all_async(s.as_bytes()).await?;
            } else {
                use std::io::Write;
                let mut out = out;
                out.write_all(s.as_bytes())?;
                out.flush()?;
            }
        } else {
            return Err(std::io::Error::other("standard output not available").into());
        }

        Ok(ExecutionResult::success())
    }
}
