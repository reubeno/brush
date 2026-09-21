//! Engine-neutral helpers for interpreting a builtin's arguments.
//!
//! The shell hands a builtin its arguments exactly as expanded. These helpers
//! implement the conventions bash's own builtins follow, so any argument-parsing
//! engine can apply them consistently.

use brush_core::CommandArg;

/// Flattens arguments into plain words. Assignments render as `name=value`.
pub(crate) fn into_words(args: Vec<CommandArg>) -> Vec<String> {
    args.into_iter()
        .map(|arg| match arg {
            CommandArg::String(s) => s,
            CommandArg::Assignment(a) => a.to_string(),
        })
        .collect()
}

/// Splits arguments into leading options and the operands after them, the way
/// bash's declaration builtins (`declare`, `export`, `local`, ...) read them.
///
/// Options are the leading string arguments that begin with `-` or `+` and are
/// longer than that one character. The first argument that is not an option
/// ends them, so `declare a=1 -x` has no options and two operands. A `--` ends
/// them too, and is dropped.
///
/// Assumes no option takes its value as a separate argument, which holds for
/// every declaration builtin.
pub(crate) fn split_leading_options(mut args: Vec<CommandArg>) -> (Vec<String>, Vec<CommandArg>) {
    let end = args
        .iter()
        .position(|arg| match arg {
            CommandArg::String(s) => s == "--" || !(s.len() > 1 && s.starts_with(['-', '+'])),
            CommandArg::Assignment(_) => true,
        })
        .unwrap_or(args.len());

    let mut operands = args.split_off(end);
    if matches!(operands.first(), Some(CommandArg::String(s)) if s == "--") {
        operands.remove(0);
    }

    (into_words(args), operands)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<CommandArg> {
        words
            .iter()
            .map(|w| CommandArg::String((*w).to_owned()))
            .collect()
    }

    fn split(words: &[&str]) -> (Vec<String>, Vec<String>) {
        let (options, operands) = split_leading_options(strings(words));
        (options, into_words(operands))
    }

    #[test]
    fn options_end_at_the_first_operand() {
        assert_eq!(
            split(&["-x", "+r", "a", "-i"]),
            (
                vec!["-x".into(), "+r".into()],
                vec!["a".into(), "-i".into()]
            )
        );
    }

    #[test]
    fn double_dash_ends_options_and_is_dropped() {
        assert_eq!(
            split(&["-x", "--", "-y"]),
            (vec!["-x".into()], vec!["-y".into()])
        );
    }

    #[test]
    fn lone_dash_and_plus_are_operands() {
        assert_eq!(split(&["-", "+"]), (vec![], vec!["-".into(), "+".into()]));
    }

    #[test]
    fn all_options_leaves_no_operands() {
        assert_eq!(split(&["-p"]), (vec!["-p".into()], vec![]));
    }
}
