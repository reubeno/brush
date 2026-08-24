use std::io::Write;

use brush_core::{ExecutionResult, builtins, escape};

/// Echo text to standard output.
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    args: Vec<String>,
}

impl builtins::Command for EchoCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let no_trailing_newline = bpaf::short('n')
            .help("Suppress the trailing newline from the output.")
            .switch();
        let interpret_backslash_escapes = bpaf::short('e')
            .help("Interpret backslash escapes in the provided text.")
            .switch();
        let no_interpret_backslash_escapes = bpaf::short('E')
            .help("Do not interpret backslash escapes in the provided text.")
            .switch();
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(EchoCommand {
            no_trailing_newline,
            interpret_backslash_escapes,
            no_interpret_backslash_escapes,
            args,
        })
    }

    fn about() -> &'static str {
        "Echo text to standard output."
    }

    fn synopsis() -> &'static str {
        "[-neE] [TOKENS]..."
    }

    fn takes_trailing_args() -> bool {
        true
    }

    fn set_trailing_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut trailing_newline = !self.no_trailing_newline;
        let mut s;
        if self.interpret_backslash_escapes && !self.no_interpret_backslash_escapes {
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
