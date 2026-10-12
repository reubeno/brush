//! Makes candidates edits of the line being completed, as readline does with the candidates
//! bash generates: quoting them, closing the quote the word being completed is in, marking
//! directories, and adding a trailing space -- whatever decides what the line says.

use std::ops::Range;

use itertools::Itertools;

use super::{
    Candidate, CandidateKind, CompleteOption, Completions, Edit, EditPrefs, GenerationOptions,
    ResolvedCandidate,
    quoting::{Quote, Quoter},
};

/// Makes candidates edits of a line, replacing the word being completed.
pub(super) struct CandidateEdits<'a> {
    /// The line being completed.
    pub line: &'a str,
    /// The byte range of the word being completed, starting at the quote it's in (if any)
    /// and running up to the cursor.
    pub word: Range<usize>,
    /// The quote the word is in, if the cursor is inside an unclosed quote.
    pub open_quote: Option<Quote>,
    /// The options in effect for the candidates.
    pub options: &'a GenerationOptions,
    /// The line editor's preferences.
    pub prefs: &'a EditPrefs,
}

impl CandidateEdits<'_> {
    /// Returns the completions of the word with `candidates`, without duplicates.
    pub fn completions(&self, candidates: Vec<ResolvedCandidate>) -> Completions {
        let quoter = Quoter {
            open_quote: self.open_quote,
            quote_file_names: !self.options.get(CompleteOption::NoQuote),
        };

        Completions {
            candidates: candidates
                .into_iter()
                .unique()
                .map(|candidate| self.candidate(&quoter, candidate))
                .collect(),
        }
    }

    /// Returns `candidate` as a candidate that completes the word.
    ///
    /// Like readline, a directory is marked with a trailing slash, if so configured. The
    /// completed word keeps the quote it's in: a candidate that needed quoting there closes
    /// it, in place of a closing quote the user already typed; anything else closes it only
    /// at the end of the line, and not after a directory, so it can be completed further.
    /// At the end of the line, a space follows, unless the spec said not to
    /// ([`CompleteOption::NoSpace`]) or the candidate is a directory.
    fn candidate(&self, quoter: &Quoter, candidate: ResolvedCandidate) -> Candidate {
        let next_char = self
            .line
            .get(self.word.end..)
            .and_then(|rest| rest.chars().next());
        let at_end_of_line = next_char.is_none();
        let is_dir = candidate.kind == CandidateKind::FileName { is_dir: true };
        let open_quote = self.open_quote.map(Quote::as_char);

        let quoted = quoter.quote(&candidate, is_dir && self.prefs.mark_directories);
        let mut replace = self.word.clone();
        let mut text = if let Some(closed) = quoted.closed {
            if let Some(q) = open_quote.filter(|&q| next_char == Some(q)) {
                replace.end += q.len_utf8();
            }
            closed
        } else {
            let mut text = quoted.text;
            if let Some(q) = open_quote.filter(|_| at_end_of_line && !is_dir) {
                text.push(q);
            }
            text
        };

        if at_end_of_line && !is_dir && !self.options.get(CompleteOption::NoSpace) {
            text.push(' ');
        }

        Candidate {
            kind: candidate.kind,
            value: candidate.text,
            edit: Edit { replace, text },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the options for candidates that are file names, or else aren't.
    fn options(file_names: bool) -> GenerationOptions {
        file_names
            .then_some(CompleteOption::FileNames)
            .into_iter()
            .collect()
    }

    /// Returns the completions of `word`, the start of `line` (which includes any opening
    /// quote it's in), with `candidates`; the rest of `line` follows the cursor.
    fn completions_of(
        candidates: Vec<ResolvedCandidate>,
        line: &str,
        word: &str,
        options: &GenerationOptions,
        quote: Option<Quote>,
        mark_directories: bool,
    ) -> Completions {
        let prefs = EditPrefs {
            mark_directories,
            ..EditPrefs::default()
        };
        let editor = CandidateEdits {
            line,
            word: 0..word.len(),
            open_quote: quote,
            options,
            prefs: &prefs,
        };
        editor.completions(candidates)
    }

    /// Returns the edits that completing `word` in `line` with `candidates` offers.
    fn offered_edits(
        candidates: &[&str],
        line: &str,
        word: &str,
        options: &GenerationOptions,
        quote: Option<Quote>,
    ) -> Vec<Edit> {
        let candidates = candidates
            .iter()
            .map(|&c| {
                if options.get(CompleteOption::FileNames) {
                    ResolvedCandidate::file_name(c)
                } else {
                    ResolvedCandidate::new(c)
                }
            })
            .collect();
        let completions = completions_of(candidates, line, word, options, quote, true);
        completions.candidates.into_iter().map(|c| c.edit).collect()
    }

    #[test]
    fn duplicate_candidates_are_dropped() {
        let completions = completions_of(
            vec![
                ResolvedCandidate::new("b"),
                ResolvedCandidate::new("a"),
                ResolvedCandidate::new("b"),
            ],
            "",
            "",
            &options(false),
            None,
            true,
        );
        let values: Vec<_> = completions
            .candidates
            .into_iter()
            .map(|c| c.value)
            .collect();
        assert_eq!(values, ["b", "a"]);
    }

    #[test]
    fn file_names_are_quoted_for_the_line_but_not_as_values() {
        let complete = |candidate: &str, word: &str, quote| {
            let completions = completions_of(
                vec![ResolvedCandidate::file_name(candidate)],
                word,
                word,
                &options(true),
                quote,
                true,
            );
            completions
                .candidates
                .into_iter()
                .map(|c| (c.edit.text, c.value))
                .collect::<Vec<_>>()
        };
        let expected = |text: &str, value: &str| vec![(text.to_owned(), value.to_owned())];

        assert_eq!(
            complete("dir/a b", "d", None),
            expected(r"dir/a\ b ", "dir/a b")
        );
        assert_eq!(
            complete("a$b", "'a", Some(Quote::Single)),
            expected("'a$b' ", "a$b")
        );
        // Completing in an open quote keeps it, closing it for non-directories.
        assert_eq!(
            complete("ab/e", "'a", Some(Quote::Single)),
            expected("'ab/e' ", "ab/e")
        );
        assert_eq!(
            complete("a\"b/", "\"a", Some(Quote::Double)),
            expected(r#""a\"b"/"#, "a\"b/")
        );
    }

    /// Like bash, `nospace` leaves out just the trailing space: a word in an open quote
    /// still gets its quote closed. Expected values were captured from bash 5.3.
    #[test]
    fn no_space_still_closes_quote() {
        for file_names in [false, true] {
            let mut options = options(file_names);
            options.set(CompleteOption::NoSpace, true);
            let edits = offered_edits(&["value"], "\"va", "\"va", &options, Some(Quote::Double));
            let texts: Vec<_> = edits.into_iter().map(|edit| edit.text).collect();
            assert_eq!(texts, ["\"value\""], "file names: {file_names}");
        }
    }

    /// Like readline, directories are marked with a trailing slash only if so configured;
    /// either way, they don't get a trailing space.
    #[test]
    fn directories_are_marked_if_configured() {
        let dir = ResolvedCandidate {
            kind: CandidateKind::FileName { is_dir: true },
            ..ResolvedCandidate::new("sub")
        };
        let complete = |mark_directories| {
            completions_of(
                vec![dir.clone()],
                "s",
                "s",
                &options(true),
                None,
                mark_directories,
            )
            .candidates
            .into_iter()
            .map(|c| (c.edit.text, c.kind))
            .collect::<Vec<_>>()
        };
        let dir_kind = CandidateKind::FileName { is_dir: true };

        assert_eq!(complete(true), [("sub/".to_owned(), dir_kind)]);
        assert_eq!(complete(false), [("sub".to_owned(), dir_kind)]);
    }

    /// Tests how a file name completed in an open quote is inserted, before the given next
    /// char in the line (`None` at its end). Expected values were captured from bash 5.3.
    #[test]
    fn file_name_in_open_quote_like_bash() {
        // Each case's expected edit text, and whether it replaces the closing quote just
        // past the cursor.
        for (candidate, quote, next_char, expected, replaces_closing_quote) in [
            // A name that needs quoting gets its quote closed, wherever it is, and a
            // directory's slash after it.
            ("it's", Quote::Double, None, r#""it's" "#, false),
            ("it's", Quote::Double, Some(' '), r#""it's""#, false),
            ("it's", Quote::Single, Some(' '), r"'it'\''s'", false),
            (
                "dir with space/",
                Quote::Double,
                None,
                r#""dir with space"/"#,
                false,
            ),
            (
                "dir with space/",
                Quote::Double,
                Some(' '),
                r#""dir with space"/"#,
                false,
            ),
            // A name that doesn't is inserted as is, and its quote is only closed at the end
            // of the line, and not after a directory.
            ("plain", Quote::Double, None, r#""plain" "#, false),
            ("plain", Quote::Double, Some(' '), r#""plain"#, false),
            ("sub/", Quote::Single, None, "'sub/", false),
            ("sub/", Quote::Single, Some(' '), "'sub/", false),
            // A name quoted in full replaces a closing quote the user already typed; one
            // inserted as is leaves it be.
            ("it's", Quote::Double, Some('"'), r#""it's""#, true),
            (
                "dir with space/",
                Quote::Double,
                Some('"'),
                r#""dir with space"/"#,
                true,
            ),
            ("plain", Quote::Double, Some('"'), r#""plain"#, false),
            ("sub/", Quote::Single, Some('\''), "'sub/", false),
        ] {
            let word = std::format!("{}x", quote.as_char());
            let line = std::format!("{word}{}", next_char.map(String::from).unwrap_or_default());
            let edits: Vec<_> = completions_of(
                vec![ResolvedCandidate::file_name(candidate)],
                &line,
                &word,
                &options(true),
                Some(quote),
                true,
            )
            .candidates
            .into_iter()
            .map(|c| c.edit)
            .collect();
            let replace = 0..word.len() + usize::from(replaces_closing_quote);
            assert_eq!(
                edits,
                [Edit {
                    replace,
                    text: expected.to_owned()
                }],
                "{candidate:?} in {quote:?}, before {next_char:?}"
            );
        }
    }
}
