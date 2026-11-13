use std::io::Write;

use brush_core::builtins::{Invocation, UsageError};
use brush_core::{ExecutionResult, builtins, escape};
use winnow_args::Args as _;

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

    /// Every word given, as given. With `xpg_echo` in POSIX mode, echo takes
    /// no options, so these are what it prints.
    #[arg(skip)]
    words: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(EchoCommand, parse = parse_echo);

/// Parses echo's options up to the first `--`, which with every word after it
/// is kept as an operand, and keeps all the words as given.
fn parse_echo(invocation: Invocation<'_>) -> Result<EchoCommand, UsageError> {
    let name = invocation.name();
    let words: Vec<String> = invocation
        .into_args()
        .into_iter()
        .map(String::from)
        .collect();
    let end = words.iter().position(|w| w == "--").unwrap_or(words.len());

    let mut this = EchoCommand::parse_words(&winnow_args::words(&words[..end]))
        .map_err(|e| brush_winnow_args_adapter::to_usage_error::<EchoCommand>(name, &e))?;
    this.args.extend_from_slice(&words[end..]);
    this.words = words;

    Ok(this)
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for EchoCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let options = context.shell.options();
        let xpg_echo = options.echo_builtin_expands_escape_sequences;

        // With xpg_echo, escapes are expanded unless -E says otherwise; in
        // POSIX mode echo then takes no options at all.
        let (args, mut trailing_newline, interpret_backslash_escapes) =
            if xpg_echo && options.posix_mode {
                (&self.words, true, true)
            } else {
                (
                    &self.args,
                    !self.no_trailing_newline,
                    self.interpret_backslash_escapes
                        || (xpg_echo && !self.no_interpret_backslash_escapes),
                )
            };

        let mut s;
        if interpret_backslash_escapes {
            s = String::new();
            for (i, arg) in args.iter().enumerate() {
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
            s = args.join(" ");
        }

        if trailing_newline {
            s.push('\n');
        }

        write!(context.stdout(), "{s}")?;
        context.stdout().flush()?;

        Ok(ExecutionResult::success())
    }
}
