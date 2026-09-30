//! Offers the completions the shell generates to the user, like readline: completing to the
//! candidates' common prefix first, if that changes the line, or else offering each of them.
//! The shell makes each candidate an edit of the line; this just chooses which to offer, and
//! how to show them.

use brush_core::completion::{CandidateKind, Completions, Edit};

/// What to offer the user when they ask for completion.
#[derive(Debug, Default)]
pub(crate) struct Offers {
    /// An edit to make right away: the only candidate's, or the candidates' common prefix.
    pub edit: Option<Edit>,
    /// The candidates to list: if there are several and no edit to make first -- or, like
    /// readline's `show-all-if-ambiguous`, even if there is.
    pub list: Vec<Offer>,
}

/// A candidate to offer the user.
#[derive(Debug)]
pub(crate) struct Offer {
    /// The edit of the line that makes the completion.
    pub edit: Edit,
    /// How to list the completion among others.
    pub display: String,
    /// Whether it completes a directory's name.
    pub is_dir: bool,
}

#[allow(
    dead_code,
    reason = "used only by the input backends, which may not be enabled"
)]
pub(crate) async fn complete_async(
    shell: &mut brush_core::Shell<impl brush_core::ShellExtensions>,
    line: &str,
    pos: usize,
    show_all_if_ambiguous: bool,
) -> Offers {
    // For now, the shell stores the line editor's preferences, as `bind` sets them.
    let prefs = shell.completion_config().edit_prefs.clone();

    let completion_future = shell.complete(line, pos, &prefs);
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

    // Intentionally ignore any errors that arise: there's then nothing to complete with.
    result.map_or_else(
        |_| Offers::default(),
        |completions| offers(completions, prefs.mark_directories, show_all_if_ambiguous),
    )
}

/// Returns what to offer from `completions`: like readline, the only candidate, or else their
/// common prefix if there's one to complete to, or else each candidate to list -- and with
/// `show_all_if_ambiguous`, both the common prefix and the candidates. Directories are
/// listed with a trailing slash if `mark_directories`.
fn offers(completions: Completions, mark_directories: bool, show_all_if_ambiguous: bool) -> Offers {
    let mut list: Vec<Offer> = completions
        .candidates
        .into_iter()
        .map(|candidate| Offer {
            display: display(&candidate.value, candidate.kind, mark_directories),
            is_dir: candidate.kind == CandidateKind::FileName { is_dir: true },
            edit: candidate.edit,
        })
        .collect();

    if list.len() == 1 {
        let edit = list.pop().map(|offer| offer.edit);
        return Offers { edit, list };
    }

    let edit = completions.common_prefix;
    if edit.is_some() && !show_all_if_ambiguous {
        list.clear();
    }
    Offers { edit, list }
}

/// Returns how to list a candidate with the given value and kind: like readline, as is, but
/// for a file name, just its last component, marked with a trailing slash if it's a
/// directory and `mark_directories`.
fn display(value: &str, kind: CandidateKind, mark_directories: bool) -> String {
    if !matches!(kind, CandidateKind::FileName { .. }) {
        return value.to_owned();
    }

    let trimmed = brush_core::sys::fs::strip_path_separator_suffix(value);
    let mut display = brush_core::sys::fs::rfind_path_separator(trimmed)
        .and_then(|index| value.get(index + 1..))
        .unwrap_or(value)
        .to_owned();

    if matches!(kind, CandidateKind::FileName { is_dir: true })
        && mark_directories
        && !brush_core::sys::fs::ends_with_path_separator(&display)
    {
        display.push('/');
    }
    display
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_listed_by_last_component() {
        let file = CandidateKind::FileName { is_dir: false };
        let dir = CandidateKind::FileName { is_dir: true };

        assert_eq!(display("dir/a b", file, true), "a b");
        assert_eq!(display("~/Docs/", dir, true), "Docs/");
        assert_eq!(display("dir/sub", dir, true), "sub/");
        assert_eq!(display("dir/sub", dir, false), "sub");
        // Anything else is listed as is.
        assert_eq!(display("a/b", CandidateKind::Other, true), "a/b");
    }
}
