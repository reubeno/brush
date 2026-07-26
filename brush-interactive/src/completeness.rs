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
        // continuation, which the tokenizer drops silently at end of input. Ask again
        // with the newline removed; the tokenizer reports an unterminated escape only
        // if that backslash was really escaping something.
        Ok(_) => ends_with_line_continuation(shell, input),
    }
}

/// Returns whether the given input ends with a backslash-newline acting as a line
/// continuation.
fn ends_with_line_continuation(
    shell: &Shell<impl brush_core::ShellExtensions>,
    input: &str,
) -> bool {
    let Some(truncated) = input.strip_suffix('\n') else {
        return false;
    };

    // Keeps the extra parse off the common path.
    if !truncated.ends_with('\\') {
        return false;
    }

    matches!(
        shell.parse_string(truncated),
        Err(brush_parser::ParseError::Tokenizing {
            inner: brush_parser::TokenizerError::UnterminatedEscapeSequence,
            position: _,
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_shell() -> Shell<brush_core::extensions::DefaultShellExtensions> {
        brush_core::Shell::builder().build().await.unwrap()
    }

    #[tokio::test]
    async fn treats_trailing_line_continuation_as_incomplete() {
        let shell = test_shell().await;

        assert!(needs_more_input_locked(&shell, "echo a\\\n"));
        assert!(needs_more_input_locked(&shell, "printf '%s' \\\n"));
        assert!(needs_more_input_locked(&shell, "echo \"quoted\" \\\n"));
        // A backslash at the very end, with no newline yet.
        assert!(needs_more_input_locked(&shell, "echo a\\"));
    }

    #[tokio::test]
    async fn treats_complete_programs_as_complete() {
        let shell = test_shell().await;

        assert!(!needs_more_input_locked(&shell, "echo a\n"));
        assert!(!needs_more_input_locked(&shell, ""));
        // An escaped backslash is a literal backslash, not an escape of the newline.
        assert!(!needs_more_input_locked(&shell, "echo a\\\\\n"));
        // A continuation that isn't at the end has already been joined.
        assert!(!needs_more_input_locked(&shell, "echo a\\\nb\n"));
        // A backslash followed by a space escapes the space, not the newline.
        assert!(!needs_more_input_locked(&shell, "echo a\\ \n"));
        assert!(!needs_more_input_locked(&shell, "echo a\\\t\n"));
    }

    #[tokio::test]
    async fn ignores_backslashes_that_do_not_escape() {
        let shell = test_shell().await;

        // Backslashes are literal inside single quotes...
        assert!(!needs_more_input_locked(&shell, "echo 'a\\'\n"));
        // ...and inside comments, which end at the newline regardless.
        assert!(!needs_more_input_locked(&shell, "echo hi # trailing\\\n"));
        assert!(!needs_more_input_locked(&shell, "# whole-line comment\\\n"));
    }

    #[tokio::test]
    async fn honors_quoting_context() {
        let shell = test_shell().await;

        // A `#` inside quotes doesn't start a comment, so these really are continuations.
        assert!(needs_more_input_locked(
            &shell,
            "echo \"# not a comment\" \\\n"
        ));
        assert!(needs_more_input_locked(
            &shell,
            "echo '# not a comment' \\\n"
        ));
        // A `#` mid-word doesn't start a comment either.
        assert!(needs_more_input_locked(&shell, "echo a#b \\\n"));
    }

    #[tokio::test]
    async fn treats_bad_syntax_as_complete_even_with_a_trailing_continuation() {
        let shell = test_shell().await;

        // More input can't repair a bad token, so don't sit waiting for it.
        assert!(!needs_more_input_locked(&shell, "echo ;;\n"));
        assert!(!needs_more_input_locked(&shell, "echo ;; \\\n"));
    }

    #[tokio::test]
    async fn treats_unfinished_constructs_as_incomplete() {
        let shell = test_shell().await;

        assert!(needs_more_input_locked(&shell, "if true\n"));
        assert!(needs_more_input_locked(&shell, "f() {\n"));
        assert!(needs_more_input_locked(&shell, "cat <<EOF\n"));
        assert!(needs_more_input_locked(&shell, "echo 'unterminated\n"));
        assert!(needs_more_input_locked(&shell, "true &&\n"));
    }
}
