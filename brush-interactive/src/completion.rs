use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use indexmap::IndexSet;

use brush_core::escape;

pub(crate) async fn complete_async(
    shell: &mut brush_core::Shell<impl brush_core::ShellExtensions>,
    line: &str,
    pos: usize,
) -> brush_core::completion::Completions {
    let working_dir = shell.working_dir().to_path_buf();

    // Intentionally ignore any errors that arise.
    let completion_future = shell.complete(line, pos);
    tokio::pin!(completion_future);

    // Wait for the completions to come back or interruption, whichever happens first.
    let result = tokio::select! {
        result = &mut completion_future => {
            result
        }
        _ = tokio::signal::ctrl_c() => {
            Err(brush_core::ErrorKind::Interrupted.into())
        },
    };

    let mut completions = result.unwrap_or_else(|_| brush_core::completion::Completions {
        insertion_index: pos,
        delete_count: 0,
        candidates: IndexSet::new(),
        options: brush_core::completion::ProcessingOptions::default(),
    });

    // Look at the line up to 'pos' to check if we're in an unterminated
    // single or double quote string.
    let mut quote_char: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if i >= pos {
            break;
        }

        if escaped {
            escaped = false;
            continue;
        }

        if let Some(q) = quote_char {
            if c == q {
                quote_char = None;
            }
        } else if c == '\\' {
            escaped = true;
        } else if c == '\'' || c == '\"' {
            quote_char = Some(c);
        }
    }

    // Store the quote context for later use when inserting completions
    completions.options.quote_char = quote_char;

    // Postprocess candidates: add directory suffix and trailing space, but do NOT
    // escape special characters here. Escaping is done only when inserting a single
    // completion (not when displaying multiple options).
    let completing_end_of_line = pos == line.len();
    completions.candidates = completions
        .candidates
        .into_iter()
        .map(|candidate| {
            postprocess_completion_candidate_for_display(
                candidate,
                &completions.options,
                working_dir.as_ref(),
                completing_end_of_line,
            )
        })
        .collect();

    completions
}

/// Postprocess a completion candidate for display purposes.
/// Adds directory suffix and trailing space, but does NOT escape special characters.
/// This is used when showing multiple completion options to the user.
fn postprocess_completion_candidate_for_display(
    mut candidate: String,
    options: &brush_core::completion::ProcessingOptions,
    working_dir: &Path,
    completing_end_of_line: bool,
) -> String {
    if options.treat_as_filenames {
        // Check if it's a directory.
        if !candidate.ends_with(std::path::MAIN_SEPARATOR) {
            let candidate_path = Path::new(&candidate);
            let abs_candidate_path = if candidate_path.is_absolute() {
                PathBuf::from(candidate_path)
            } else {
                working_dir.join(candidate_path)
            };

            if abs_candidate_path.is_dir() {
                candidate.push(std::path::MAIN_SEPARATOR);
            }
        }
    }

    if completing_end_of_line && !options.no_trailing_space_at_end_of_line {
        if !options.treat_as_filenames || !candidate.ends_with(std::path::MAIN_SEPARATOR) {
            candidate.push(' ');
        }
    }

    candidate
}

/// Escape a completion candidate for insertion into the command line.
/// This applies appropriate quoting based on the quote context.
pub(crate) fn escape_completion_for_insertion<'a>(
    candidate: &'a str,
    options: &brush_core::completion::ProcessingOptions,
) -> Cow<'a, str> {
    if options.treat_as_filenames && !options.no_autoquote_filenames {
        let quote_mode = match options.quote_char {
            Some('\'') => escape::QuoteMode::SingleQuote,
            Some('\"') => escape::QuoteMode::DoubleQuote,
            _ => escape::QuoteMode::BackslashEscape,
        };

        escape::quote_if_needed(candidate, quote_mode)
    } else {
        Cow::Borrowed(candidate)
    }
}
