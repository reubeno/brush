use std::path::{Path, PathBuf};

use crate::trace_categories;

pub(crate) async fn complete_async(
    shell: &mut brush_core::Shell,
    line: &str,
    pos: usize,
) -> brush_core::completion::Completions {
    let working_dir = shell.working_dir.clone();

    // Intentionally ignore any errors that arise.
    let completion_future = shell.get_completions(line, pos);
    tokio::pin!(completion_future);

    // Wait for the completions to come back or interruption, whichever happens first.
    let result = loop {
        tokio::select! {
            result = &mut completion_future => {
                break result;
            }
            _ = tokio::signal::ctrl_c() => {
            },
        }
    };

    let mut completions = result.unwrap_or_else(|_| brush_core::completion::Completions {
        insertion_index: pos,
        delete_count: 0,
        candidates: vec![],
        options: brush_core::completion::ProcessingOptions::default(),
    });

    let completing_end_of_line = pos == line.len();
    if completions.options.treat_as_filenames {
        for candidate in &mut completions.candidates {
            // Check if it's a directory.
            if !candidate.ends_with(std::path::MAIN_SEPARATOR) {
                let candidate_path = Path::new(candidate);
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
    }
    if completions.options.no_autoquote_filenames {
        tracing::debug!(target: trace_categories::COMPLETION, "don't autoquote filenames");
    }
    if completing_end_of_line && !completions.options.no_trailing_space_at_end_of_line {
        for candidate in &mut completions.candidates {
            if !completions.options.treat_as_filenames
                || !candidate.ends_with(std::path::MAIN_SEPARATOR)
            {
                candidate.push(' ');
            }
        }
    }

    completions
}
