//! `fc` builtin: `FcCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

/// Returns whether the given argument looks like a negative number; these are
/// treated as operands since they specify offsets relative to the end of
/// history rather than options.
fn is_negative_number(arg: &str) -> bool {
    arg.strip_prefix('-')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// Returns the effective history count (excluding the fc command itself).
fn effective_history_count(history: &brush_core::history::History) -> usize {
    history.count().saturating_sub(1)
}

fn run_bpaf_parser<T: crate::args::bpaf_support::BpafArgs>(args: &[String]) -> Result<T, ArgsError> {
    crate::args::bpaf_support::run_parser::<T>(args)
}

fn render_bpaf_failure(failure: bpaf::ParseFailure) -> ArgsError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => ArgsError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => ArgsError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => ArgsError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use brush_core::builtins::Command as _;

    fn new_from(args: &[&str]) -> Result<FcCommand, ArgsError> {
        FcCommand::new(std::iter::once("fc".to_string()).chain(args.iter().map(|s| s.to_string())))
    }

    #[test]
    fn test_negative_indices_as_operands() -> anyhow::Result<()> {
        let cmd = new_from(&["-l", "-3", "-1"])?;
        assert!(cmd.list);
        assert_eq!(cmd.first.as_deref(), Some("-3"));
        assert_eq!(cmd.last.as_deref(), Some("-1"));

        Ok(())
    }

    #[test]
    fn test_options_and_operands() -> anyhow::Result<()> {
        let cmd = new_from(&["-e", "vim", "10", "20"])?;
        assert_eq!(cmd.editor.as_deref(), Some("vim"));
        assert_eq!(cmd.first.as_deref(), Some("10"));
        assert_eq!(cmd.last.as_deref(), Some("20"));

        Ok(())
    }

    #[test]
    fn test_substitution_spec() -> anyhow::Result<()> {
        let cmd = new_from(&["-s", "ech=echo"])?;
        assert!(cmd.substitute);
        assert_eq!(cmd.first.as_deref(), Some("ech=echo"));
        assert_eq!(cmd.last, None);

        Ok(())
    }
}

/// Process command history list.
pub(crate) struct FcCommand {
    /// List commands instead of editing them.
    pub(super) list: bool,

    /// Suppress line numbers when listing.
    pub(super) no_line_numbers: bool,

    /// Reverse the order of commands.
    pub(super) reverse: bool,

    /// Re-execute command after substitution (old=new format).
    pub(super) substitute: bool,

    /// Editor to use (only relevant when not listing or substituting).
    // N.B. Editor mode is not yet implemented, so this is only surfaced
    // through the option parser and help text.
    #[cfg_attr(not(test), expect(dead_code))]
    pub(super) editor: Option<String>,

    /// First command in range (number or string prefix).
    pub(super) first: Option<String>,

    /// Last command in range (number or string prefix).
    pub(super) last: Option<String>,
}

impl crate::args::bpaf_support::BpafArgs for FcCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let list = bpaf::short('l')
            .help("List commands instead of editing them.")
            .switch();
        let no_line_numbers = bpaf::short('n')
            .help("Suppress line numbers when listing.")
            .switch();
        let reverse = bpaf::short('r')
            .help("Reverse the order of commands.")
            .switch();
        let substitute = bpaf::short('s')
            .help("Re-execute command after substitution (old=new format).")
            .switch();
        let editor = bpaf::short('e')
            .help("Editor to use (only relevant when not listing or substituting).")
            .argument::<String>("ENAME")
            .optional();
        let first = bpaf::pure(None);
        let last = bpaf::pure(None);

        bpaf::construct!(FcCommand {
            list,
            no_line_numbers,
            reverse,
            substitute,
            editor,
            first,
            last,
        })
    }
fn about() -> &'static str {
        "Process command history list."
    }
fn synopsis() -> &'static str {
        "[-lnrs] [-e ENAME] [FIRST [LAST]]"
    }
fn takes_trailing_args() -> bool {
        true
    }
fn value_taking_short_options() -> &'static str {
        "e"
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        let mut iter = args.into_iter();
        if let Some(first) = iter.next() {
            self.first = Some(first);
        }
        if let Some(last) = iter.next() {
            self.last = Some(last);
        }
    }
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();

        let mut options = Vec::new();
        let mut trailing = Vec::new();

        // N.B. The first argument is the command name itself.
        let mut iter = args.into_iter().skip(1);
        let mut pending_value = false;
        while let Some(arg) = iter.next() {
            if pending_value {
                // This token is the value of a preceding value-taking option.
                options.push(arg);
                pending_value = false;
                continue;
            }

            if arg == "--" {
                trailing.extend(iter);
                break;
            }

            if !arg.starts_with('-') || arg == "-" {
                // An operand; everything from here on is captured verbatim.
                trailing.push(arg);
                trailing.extend(iter);
                break;
            }

            if is_negative_number(&arg) {
                // A negative history index (an operand).
                trailing.push(arg);
                continue;
            }

            if let Some(group) = arg.strip_prefix('-').filter(|g| !g.starts_with('-')) {
                let chars: Vec<char> = group.chars().collect();
                for (j, c) in chars.iter().enumerate() {
                    match c {
                        'e' => {
                            pending_value = j == chars.len() - 1;
                            break;
                        }
                        'l' | 'n' | 'r' | 's' => {}
                        _ => break,
                    }
                }
            }

            options.push(arg);
        }

        let mut command = run_bpaf_parser::<Self>(&options)?;
        command.set_trailing_args(trailing);

        Ok(command)
    
    }
}

impl FromArgs for FcCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for FcCommand {
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
