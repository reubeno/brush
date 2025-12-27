use std::path::{Path, PathBuf};

use indexmap::IndexSet;

use crate::trace_categories;
use brush_core::escape;

#[allow(dead_code)]
pub(crate) async fn complete_async(
    shell: &mut brush_core::Shell,
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

    // Look at the line upto 'pos' to check if we're in an unterminated
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

    // TODO(completion): Consider optimizing this out when not needed?
    let completing_end_of_line = pos == line.len();
    completions.candidates = completions
        .candidates
        .into_iter()
        .map(|candidate| {
            postprocess_completion_candidate(
                candidate,
                &completions.options,
                working_dir.as_ref(),
                completing_end_of_line,
                quote_char,
            )
        })
        .collect();

    completions
}

#[allow(dead_code)]
fn postprocess_completion_candidate(
    mut candidate: String,
    options: &brush_core::completion::ProcessingOptions,
    working_dir: &Path,
    completing_end_of_line: bool,
    quote_char: Option<char>,
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

        if !options.no_autoquote_filenames {
            let quote_mode = match quote_char {
                Some(q) => {
                    if q == '\'' {
                        escape::QuoteMode::SingleQuote
                    } else {
                        escape::QuoteMode::DoubleQuote
                    }
                }
                None => escape::QuoteMode::BackslashEscape,
            };

            candidate = escape::quote_if_needed(&candidate, quote_mode).to_string();
        }
    }
    if options.no_autoquote_filenames {
        tracing::debug!(target: trace_categories::COMPLETION, "unimplemented: don't autoquote filenames");
    }
    if completing_end_of_line && !options.no_trailing_space_at_end_of_line {
        if !options.treat_as_filenames || !candidate.ends_with(std::path::MAIN_SEPARATOR) {
            candidate.push(' ');
        }
    }

    candidate
}
