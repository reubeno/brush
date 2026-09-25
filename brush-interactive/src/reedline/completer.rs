use nu_ansi_term::{Color, Style};
use std::borrow::BorrowMut;
use unicode_segmentation::UnicodeSegmentation;

use crate::{completion, refs};

pub(crate) struct ReedlineCompleter<SE: brush_core::ShellExtensions> {
    pub shell: refs::ShellRef<SE>,
}

impl<SE: brush_core::ShellExtensions> reedline::Completer for ReedlineCompleter<SE> {
    fn complete(&mut self, line: &str, pos: usize) -> reedline::CompletionResult {
        let suggestions = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.complete_async(line, pos))
        });
        reedline::CompletionResult::fresh(suggestions)
    }
}

impl<SE: brush_core::ShellExtensions> ReedlineCompleter<SE> {
    async fn complete_async(&self, line: &str, pos: usize) -> Vec<reedline::Suggestion> {
        let mut shell_guard = self.shell.lock().await;
        let shell = shell_guard.borrow_mut().as_mut();
        let offers = completion::complete_async(shell, line, pos, false).await;

        // We're done with the shell, so drop it eagerly.
        drop(shell_guard);

        let offers = match offers.edit {
            // A lone suggestion is made at once, so its display is never shown.
            Some(edit) => vec![completion::Offer {
                display: edit.text.clone(),
                edit,
                is_dir: false,
            }],
            None => offers.list,
        };

        let shared = shared_graphemes(offers.iter().map(|offer| offer.display.as_str()));
        offers
            .into_iter()
            .map(|offer| Self::to_suggestion(offer, shared))
            .collect()
    }

    /// Returns `offer` as a suggestion, whose display has its first `shared` graphemes
    /// highlighted.
    fn to_suggestion(
        completion::Offer {
            edit,
            display,
            is_dir,
        }: completion::Offer,
        shared: usize,
    ) -> reedline::Suggestion {
        let mut style = Style::new();
        if is_dir {
            style = style.fg(Color::Green);
        }

        let brush_core::completion::Edit { replace, text } = edit;

        reedline::Suggestion {
            value: text,
            description: None,
            style: Some(style),
            extra: None,
            span: reedline::Span {
                start: replace.start,
                end: replace.end,
            },
            // Like readline's `colored-completion-prefix`, highlight the part of the listed
            // candidates that they share. (Otherwise, the menu highlights where it finds the
            // text the edit replaces, which a file name's display, its last component,
            // doesn't have.)
            match_indices: Some((0..shared).collect()),
            display_override: Some(display),
            // The edit's text already has any trailing space.
            append_whitespace: false,
        }
    }
}

/// Returns how many graphemes all of `displays` start with.
fn shared_graphemes<'a>(mut displays: impl Iterator<Item = &'a str>) -> usize {
    let Some(first) = displays.next() else {
        return 0;
    };
    let first: Vec<_> = first.graphemes(true).collect();

    displays.fold(first.len(), |shared, display| {
        display
            .graphemes(true)
            .zip(&first)
            .take_while(|(a, b)| a == *b)
            .count()
            .min(shared)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes_shared_by_displays() {
        let shared = |displays: &[&str]| shared_graphemes(displays.iter().copied());

        assert_eq!(shared(&["foo", "fob"]), 2);
        assert_eq!(shared(&["sub/", "sua"]), 2);
        assert_eq!(shared(&["a", "b"]), 0);
        assert_eq!(shared(&["abc"]), 3);
        assert_eq!(shared(&[]), 0);
        // A char with a combining mark is one grapheme, which differs from the bare char.
        assert_eq!(shared(&["e\u{301}x", "e\u{301}y"]), 1);
        assert_eq!(shared(&["e\u{301}x", "ex"]), 0);
    }
}
