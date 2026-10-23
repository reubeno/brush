use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins, escape};

/// Echo text to standard output.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct EchoCommand {
    /// Suppress the trailing newline from the output.
    #[arg(short = 'n')]
    no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[arg(short = 'e')]
    interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    #[arg(short = 'E')]
    no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(
    EchoCommand,
    parse = |invocation: builtins::Invocation<'_>| Ok(Self::parse_args(invocation))
);

impl EchoCommand {
    /// Parses `echo`'s arguments the way bash does. Leading words made up of
    /// `-` and only the letters `n`, `e` and `E` are options. They can repeat,
    /// and when both `-e` and `-E` are given the last one wins. The first word
    /// that is not such an option, and every word after it, is echoed as is.
    fn parse_args(invocation: builtins::Invocation<'_>) -> Self {
        let mut this = Self {
            no_trailing_newline: false,
            interpret_backslash_escapes: false,
            no_interpret_backslash_escapes: false,
            args: vec![],
        };

        let mut args = invocation.into_args().into_iter().map(String::from);
        for arg in args.by_ref() {
            let is_option = arg
                .strip_prefix('-')
                .is_some_and(|flags| !flags.is_empty() && flags.chars().all(|c| "neE".contains(c)));

            if !is_option {
                this.args.push(arg);
                break;
            }

            for c in arg.chars().skip(1) {
                match c {
                    'n' => this.no_trailing_newline = true,
                    'e' => {
                        this.interpret_backslash_escapes = true;
                        this.no_interpret_backslash_escapes = false;
                    }
                    'E' => {
                        this.interpret_backslash_escapes = false;
                        this.no_interpret_backslash_escapes = true;
                    }
                    _ => {}
                }
            }
        }
        this.args.extend(args);

        this
    }
}

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

        write!(context.stdout(), "{s}")?;
        context.stdout().flush()?;

        Ok(ExecutionResult::success())
    }
}
