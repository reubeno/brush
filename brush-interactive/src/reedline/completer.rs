use nu_ansi_term::{Color, Style};
use std::borrow::BorrowMut;

use crate::{completion, refs};

pub(crate) struct ReedlineCompleter {
    pub shell: refs::ShellRef,
}

impl reedline::Completer for ReedlineCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<reedline::Suggestion> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.complete_async(line, pos))
        })
    }
}

impl ReedlineCompleter {
    async fn complete_async(&self, line: &str, pos: usize) -> Vec<reedline::Suggestion> {
        let mut shell_guard = self.shell.lock().await;
        let shell = shell_guard.borrow_mut().as_mut();
        let completions = completion::complete_async(shell, line, pos).await;

        // We're done with the shell, so drop it eagerly.
        drop(shell_guard);

        let insertion_index = completions.insertion_index;
        let delete_count = completions.delete_count;
        let options = completions.options;

        completions
            .candidates
            .into_iter()
            .map(|candidate| {
                Self::to_suggestion(line, candidate, insertion_index, delete_count, &options)
            })
            .collect()
    }

    #[allow(
        clippy::string_slice,
        reason = "all indices + counts are expected to be at char boundaries"
    )]
    fn to_suggestion(
        line: &str,
        mut candidate: String,
        mut insertion_index: usize,
        mut delete_count: usize,
        options: &brush_core::completion::ProcessingOptions,
    ) -> reedline::Suggestion {
        let mut style = Style::new();

        // Special handling for filename completions.
        if options.treat_as_filenames {
            if candidate.ends_with(std::path::MAIN_SEPARATOR) {
                style = style.fg(Color::Green);
            }

            if insertion_index + delete_count <= line.len() {
                let removed = &line[insertion_index..insertion_index + delete_count];
                if let Some(last_sep_index) = removed.rfind(std::path::MAIN_SEPARATOR) {
                    if candidate.starts_with(removed) {
                        candidate = candidate.split_off(last_sep_index + 1);
                        insertion_index += last_sep_index + 1;
                        delete_count -= last_sep_index + 1;
                    }
                }
            }
        }

        // See if there's whitespace at the end.
        let append_whitespace = candidate.ends_with(' ');
        if append_whitespace {
            candidate.pop();
        }

        reedline::Suggestion {
            value: candidate,
            description: None,
            style: Some(style),
            extra: None,
            span: reedline::Span {
                start: insertion_index,
                end: insertion_index + delete_count,
            },
            match_indices: None,
            append_whitespace,
        }
    }
}
