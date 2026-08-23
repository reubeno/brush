//! Deciding whether accumulated input forms a complete shell program.
//!
//! Input backends read a line at a time and need to know when they have enough to hand
//! over for execution.

use brush_core::Shell;

/// Returns whether the given accumulated input is an incomplete shell program, i.e.
/// whether more input must be read before it can be executed.
///
/// Acquires the shell lock without blocking. Input is read between commands, with no
/// other task holding the lock, so acquisition is expected to succeed; blocking instead
/// isn't an option because backends also run on single-threaded runtimes.
///
/// # Arguments
///
/// * `shell_ref` - The shell whose parsing options should be used.
/// * `input` - The input accumulated so far.
#[allow(dead_code)]
pub(crate) fn needs_more_input(
    shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
    input: &str,
) -> bool {
    let Ok(shell) = shell_ref.try_lock() else {
        tracing::warn!("shell was unexpectedly busy; treating input as complete");
        return false;
    };

    needs_more_input_locked(&shell, input)
}

/// Returns whether more input is needed, given an already-acquired shell.
#[allow(dead_code)]
fn needs_more_input_locked(shell: &Shell<impl brush_core::ShellExtensions>, input: &str) -> bool {
    match shell.parse_string(input) {
        // Mid-token: unclosed quotes, unterminated here documents, and the like.
        Err(brush_parser::ParseError::Tokenizing { inner, position: _ })
            if inner.is_incomplete() =>
        {
            true
        }
        // Ran out of tokens partway through a construct; more input may complete it.
        Err(brush_parser::ParseError::ParsingAtEndOfInput) => true,
        // A bad token at a specific position stays bad no matter what follows it.
        Err(_) => false,
        // Parsed cleanly. One catch: a trailing backslash-newline is a line
        // continuation, and a lone trailing backslash means more input is
        // expected. Probe the tokenizer directly rather than going through
        // `parse_string`: the tokenizer is shared by all parser implementations,
        // whereas e.g. the string-based winnow parser never produces lexical
        // errors for these inputs.
        Ok(_) => ends_with_live_trailing_backslash(shell, input),
    }
}

/// Returns whether the given input ends with a backslash acting as a line
/// continuation (`\`+newline), or dangling at the end of input awaiting
/// continuation.
///
/// The backslash must be "live": unquoted, unescaped, and outside comments.
fn ends_with_live_trailing_backslash(
    shell: &Shell<impl brush_core::ShellExtensions>,
    input: &str,
) -> bool {
    // When the input ends with a newline, a `\` just before it can only be a
    // continuation if removing the newline leaves the backslash escaping
    // something; tokenize the stripped text and let the tokenizer tell us.
    let candidate = input.strip_suffix('\n').unwrap_or(input);

    // Keeps the extra tokenize off the common path.
    if !candidate.ends_with('\\') {
        return false;
    }

    matches!(
        brush_parser::tokenize_str_with_options(
            candidate,
            &shell.parser_options().tokenizer_options(),
        ),
        Err(brush_parser::TokenizerError::UnterminatedEscapeSequence)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // The winnow parser variant exists whenever brush-parser is built with its
    // default features, as is the case for every in-tree consumer.
    const PARSER_IMPLS: &[brush_parser::ParserImpl] = &[
        brush_parser::ParserImpl::Peg,
        brush_parser::ParserImpl::Winnow,
    ];

    async fn test_shell(
        parser_impl: brush_parser::ParserImpl,
    ) -> Shell<brush_core::extensions::DefaultShellExtensions> {
        brush_core::Shell::builder()
            .parser(parser_impl)
            .build()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn treats_trailing_line_continuation_as_incomplete() {
        for parser_impl in PARSER_IMPLS {
            let shell = test_shell(*parser_impl).await;

            assert!(
                needs_more_input_locked(&shell, "echo a\\\n"),
                "{parser_impl:?}"
            );
            assert!(
                needs_more_input_locked(&shell, "printf '%s' \\\n"),
                "{parser_impl:?}"
            );
            assert!(
                needs_more_input_locked(&shell, "echo \"quoted\" \\\n"),
                "{parser_impl:?}"
            );
            // A backslash at the very end, with no newline yet.
            assert!(
                needs_more_input_locked(&shell, "echo a\\"),
                "{parser_impl:?}"
            );
        }
    }

    #[tokio::test]
    async fn treats_complete_programs_as_complete() {
        for parser_impl in PARSER_IMPLS {
            let shell = test_shell(*parser_impl).await;

            assert!(!needs_more_input_locked(&shell, "echo a\n"));
            assert!(!needs_more_input_locked(&shell, ""));
            // An escaped backslash is a literal backslash, not an escape of the newline.
            assert!(
                !needs_more_input_locked(&shell, "echo a\\\\\n"),
                "{parser_impl:?}"
            );
            // A continuation that isn't at the end has already been joined.
            assert!(
                !needs_more_input_locked(&shell, "echo a\\\nb\n"),
                "{parser_impl:?}"
            );
            // A backslash followed by a space escapes the space, not the newline.
            assert!(!needs_more_input_locked(&shell, "echo a\\ \n"));
            assert!(!needs_more_input_locked(&shell, "echo a\\\t\n"));
        }
    }

    #[tokio::test]
    async fn ignores_backslashes_that_do_not_escape() {
        for parser_impl in PARSER_IMPLS {
            let shell = test_shell(*parser_impl).await;

            // Backslashes are literal inside single quotes...
            assert!(
                !needs_more_input_locked(&shell, "echo 'a\\'\n"),
                "{parser_impl:?}"
            );
            // ...and inside comments, which end at the newline regardless.
            assert!(
                !needs_more_input_locked(&shell, "echo hi # trailing\\\n"),
                "{parser_impl:?}"
            );
            assert!(
                !needs_more_input_locked(&shell, "# whole-line comment\\\n"),
                "{parser_impl:?}"
            );
        }
    }

    #[tokio::test]
    async fn honors_quoting_context() {
        for parser_impl in PARSER_IMPLS {
            let shell = test_shell(*parser_impl).await;

            // A `#` inside quotes doesn't start a comment, so these really are continuations.
            assert!(
                needs_more_input_locked(&shell, "echo \"# not a comment\" \\\n"),
                "{parser_impl:?}"
            );
            assert!(
                needs_more_input_locked(&shell, "echo '# not a comment' \\\n"),
                "{parser_impl:?}"
            );
            // A `#` mid-word doesn't start a comment either.
            assert!(
                needs_more_input_locked(&shell, "echo a#b \\\n"),
                "{parser_impl:?}"
            );
        }
    }

    #[tokio::test]
    async fn treats_bad_syntax_as_complete_even_with_a_trailing_continuation() {
        for parser_impl in PARSER_IMPLS {
            let shell = test_shell(*parser_impl).await;

            // More input can't repair a bad token, so don't sit waiting for it.
            assert!(!needs_more_input_locked(&shell, "echo ;;\n"));
            assert!(
                !needs_more_input_locked(&shell, "echo ;; \\\n"),
                "{parser_impl:?}"
            );
        }
    }

    #[tokio::test]
    async fn treats_unfinished_constructs_as_incomplete() {
        for parser_impl in PARSER_IMPLS {
            let shell = test_shell(*parser_impl).await;

            assert!(
                needs_more_input_locked(&shell, "if true\n"),
                "{parser_impl:?}"
            );
            assert!(
                needs_more_input_locked(&shell, "f() {\n"),
                "{parser_impl:?}"
            );
            assert!(
                needs_more_input_locked(&shell, "cat <<EOF\n"),
                "{parser_impl:?}"
            );
            assert!(
                needs_more_input_locked(&shell, "echo 'unterminated\n"),
                "{parser_impl:?}"
            );
            assert!(
                needs_more_input_locked(&shell, "true &&\n"),
                "{parser_impl:?}"
            );
        }
    }
}
