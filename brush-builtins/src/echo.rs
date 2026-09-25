use std::io::Write;

use brush_core::{ExecutionResult, builtins, escape};
use usage::Cli;

/// Echo text to standard output.
#[derive(Cli)]
#[usage(
    bin = "echo",
    unknown_flags = "value",
    disable_help_flag,
    disable_version_flag
)]
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    #[usage(short = 'n')]
    no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[usage(short = 'e')]
    interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    #[usage(short = 'E')]
    no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[usage(arg, trailing_var_arg)]
    args: Vec<String>,
}

brush_builtin_usage::usage_builtin!(EchoCommand, trailing_args = args);

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
