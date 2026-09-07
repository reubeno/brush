//! `command` builtin: `CommandCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Directly invokes an external command, without going through typical search order.
#[derive(Default)]
pub(crate) struct CommandCommand {
    /// Use default PATH value.
    pub(crate) use_default_path: bool,

    /// Display a short description of the command.
    pub(crate) print_description: bool,

    /// Display a more verbose description of the command.
    pub(crate) print_verbose_description: bool,

    /// Command and arguments.
    pub(crate) command_and_args: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for CommandCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let use_default_path = bpaf::short('p').help("Use default PATH value.").switch();
        let print_description = bpaf::short('v')
            .help("Display a short description of the command.")
            .switch();
        let print_verbose_description = bpaf::short('V')
            .help("Display a more verbose description of the command.")
            .switch();
        let command_and_args = bpaf::pure(Vec::new());

        bpaf::construct!(CommandCommand {
            use_default_path,
            print_description,
            print_verbose_description,
            command_and_args,
        })
    }
fn about() -> &'static str {
        "Directly invokes an external command, without going through typical search order."
    }
fn synopsis() -> &'static str {
        "[-pvV] [COMMAND [ARG]...]"
    }
fn takes_trailing_args() -> bool {
        true
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        // N.B. Match clap's behavior here: clap consumes a bare leading `--`
        // as its separator convention instead of delivering it among the
        // positionals, so bash's `command -- cmd` resolves `cmd`, not `--`.
        let mut args = args;
        if args.first().map(String::as_str) == Some("--") {
            args.remove(0);
        }
        self.command_and_args = args;
    }
}

impl FromArgs for CommandCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        let mut command: Self =
            crate::args::bpaf_support::BpafArgs::from_words(words)?;

        // N.B. bash gives `-v`/`-V` last-wins semantics, but the switches
        // above only record presence. When both fired, resolve the tie from
        // the raw option words in order; clustered spellings (e.g. `-vV`)
        // count left-to-right, matching the parser's option-zone boundary.
        if command.print_description && command.print_verbose_description {
            // N.B. words[0] is the command name itself.
            let args: Vec<String> = words.iter().skip(1).cloned().collect();
            let (options, _) = crate::args::bpaf_support::split_option_section(&args, "", &[]);

            let mut last_is_verbose = false;
            for option in &options {
                if let Some(group) = option.strip_prefix('-') {
                    for c in group.chars() {
                        if c == 'v' {
                            last_is_verbose = false;
                        } else if c == 'V' {
                            last_is_verbose = true;
                        }
                    }
                }
            }

            command.print_description = !last_is_verbose;
            command.print_verbose_description = last_is_verbose;
        }

        Ok(command)
    }
}

impl builtins::Command for CommandCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
