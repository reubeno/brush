use std::borrow::BorrowMut;

use crate::completion;

use super::refs;

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
        let insertion_index = completions.insertion_index;
        let delete_count = completions.delete_count;

        completions
            .candidates
            .into_iter()
            .map(|candidate| Self::to_suggestion(candidate, insertion_index, delete_count))
            .collect()
    }

    fn to_suggestion(
        candidate: String,
        insertion_index: usize,
        delete_count: usize,
    ) -> reedline::Suggestion {
        let mut style = nu_ansi_term::Style::new().dimmed();
        if candidate.ends_with(std::path::MAIN_SEPARATOR) {
            style = style.fg(nu_ansi_term::Color::Green);
        }

        reedline::Suggestion {
            value: candidate,
            description: None, // TODO: fill in description
            style: Some(style),
            extra: None,
            span: reedline::Span {
                start: insertion_index,
                end: insertion_index + delete_count,
            },
            append_whitespace: false, // TODO: compute this
        }
    }
}
