use std::io::Write;

use brush_core::{ExecutionResult, builtins, escape};

/// Echo text to standard output.
///
/// A word is an option only if every letter is one echo knows (`-nx` is text),
/// and the first operand ends the options; a leading `--` is text too.
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
    #[arg(positional, value_name = "arg", double_dash = "preserve", stop_flags)]
    args: Vec<String>,
}

brush_builtin_winnow::winnow_builtin!(EchoCommand, trailing_args = args);

impl builtins::Command for EchoCommand {
    type Error = brush_core::Error;

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
