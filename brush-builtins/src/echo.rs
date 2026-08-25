use std::io::Write;

use brush_core::argmodel::{ArgSpec, CommandSpec, ParsedValues};
use brush_core::{ExecutionResult, builtins};

/// Echo text to standard output.
pub(crate) struct EchoCommand {
    no_trailing_newline: bool,
    interpret_backslash_escapes: bool,
    no_interpret_backslash_escapes: bool,
    args: Vec<String>,
}

const ID_NO_NEWLINE: &str = "no_trailing_newline";
const ID_INTERPRET: &str = "interpret_backslash_escapes";
const ID_NO_INTERPRET: &str = "no_interpret_backslash_escapes";

static ECHO_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag(
            ID_NO_NEWLINE,
            &['n'],
            &[],
            "Suppress the trailing newline from the output.",
        ),
        ArgSpec::flag(
            ID_INTERPRET,
            &['e'],
            &[],
            "Interpret backslash escapes in the provided text.",
        ),
        ArgSpec::flag(
            ID_NO_INTERPRET,
            &['E'],
            &[],
            "Do not interpret backslash escapes in the provided text.",
        ),
    ],
    positionals: &[],
};

impl builtins::SpecCommand for EchoCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        &ECHO_SPEC
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            no_trailing_newline: values.flag(ID_NO_NEWLINE),
            interpret_backslash_escapes: values.flag(ID_INTERPRET),
            no_interpret_backslash_escapes: values.flag(ID_NO_INTERPRET),
            args: values.trailing().to_vec(),
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

                let (expanded_arg, keep_going) = brush_core::escape::expand_backslash_escapes(
                    arg.as_str(),
                    brush_core::escape::EscapeExpansionMode::EchoBuiltin,
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
