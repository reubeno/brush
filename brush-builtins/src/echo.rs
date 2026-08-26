use std::io::Write;

use brush_core::{ExecutionContext, ExecutionResult, ShellExtensions, escape};

// Selects the engine-specific argument implementation for this builtin
// according to the active `parser-*` feature; see `arg_impl!`.
arg_impl!(EchoCommand);

/// Echo text to standard output.
///
/// N.B. Argument definitions and binding live in the per-engine modules
/// (`echo/{clap,bpaf,usage}.rs`); this file holds only the command's logic.
// N.B. Desugared-async shape mirrors [`brush_core::builtins::Command::execute`].
#[expect(clippy::unused_async)]
async fn execute<SE: ShellExtensions>(
    command: &EchoCommand,
    context: ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, brush_core::Error> {
    let mut trailing_newline = !command.no_trailing_newline;
    let mut s;
    if command.interpret_backslash_escapes {
        s = String::new();
        for (i, arg) in command.args.iter().enumerate() {
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
        s = command.args.join(" ");
    }

    if trailing_newline {
        s.push('\n');
    }

    write!(context.stdout(), "{s}")?;
    context.stdout().flush()?;

    Ok(ExecutionResult::success())
}
