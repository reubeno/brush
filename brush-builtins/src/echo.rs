use std::io::Write;

use brush_core::{ExecutionResult, builtins, escape};

/// Echo text to standard output.
///
/// N.B. Engine-free: argument definitions and help rendering live in the
/// per-engine argument modules (e.g., `args::clap`).
#[derive(Default)]
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    pub(crate) no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    pub(crate) interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    // N.B. Parsed for parity with bash's `-E`; consumed by clap's generated
    // binding code during migration.
    #[expect(dead_code)]
    pub(crate) no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    pub(crate) args: Vec<String>,
}

impl builtins::Command for EchoCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::clap::echo_help(name, &content_type, options)
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
