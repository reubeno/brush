//! Field splitting: breaking the results of expansion into words on `IFS`.
//!
//! Bash treats the characters in `IFS` as two kinds of delimiter:
//!
//! - *IFS whitespace* (space, tab, newline and the rest of the C locale's `isspace` set)
//!   is "soft". A run of it, however long, is one separator, and any of it at the start
//!   or end of a value is simply dropped. `"  a   b  "` splits to `a` and `b`.
//! - Every other character in `IFS` is "hard". Each occurrence is a separator in its
//!   own right, so two in a row enclose an empty field, and one at the very start leaves
//!   an empty field in front of it. With `IFS=:`, `a::b` is `a`, an empty field, `b`.
//!
//! The two mix by absorption: whitespace on either side of a hard delimiter belongs to
//! that delimiter, so `a : b` under `IFS=': '` is two fields, not three. A hard delimiter
//! at the very end closes the field before it but does not open an empty one after it.

use super::{Expansion, ExpansionPiece, WordField};

/// Where the splitter is relative to the field boundaries it has emitted.
enum SplitState {
    /// A field is open: at least one piece has gone into it. That piece may be an
    /// unquoted character or an unsplittable (quoted) piece, which can be empty, as in
    /// `""`. The next delimiter closes the field; anything else extends it.
    InField,
    /// No field is open. The last delimiter was whitespace, so the run can still absorb
    /// one hard delimiter without closing an empty field.
    AfterWhitespace,
    /// No field is open, and the run cannot absorb another hard delimiter. This is the
    /// initial state and the state right after a hard delimiter. A hard delimiter here
    /// closes an empty field.
    AfterDelimiter,
}

/// Returns whether `c` is IFS whitespace, i.e. a member of the C locale's `isspace` set,
/// which is what bash consults.
///
/// Neither standard predicate matches that set: [`char::is_ascii_whitespace`] leaves out
/// vertical tab, and [`char::is_whitespace`] takes in the Unicode spaces (U+00A0 and
/// friends), which bash treats as hard delimiters.
const fn is_ifs_whitespace(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r'
            | '\u{0b}' // vertical tab
            | '\u{0c}' // form feed
    )
}

/// Splits an expansion's fields on the characters in `ifs`, following bash's rules
/// (see the module docs). Only [`ExpansionPiece::Splittable`] text is examined;
/// quoted text and the word's own literal text always stay in the field they are in.
pub(super) fn split_fields(ifs: &str, expansion: Expansion) -> Vec<WordField> {
    let mut fields: Vec<WordField> = vec![];
    let mut current_field = WordField::new();

    // Each incoming field is split on its own, so a delimiter run never reaches across
    // the boundary between two of them (e.g. two elements of `${arr[@]}`).
    for existing_field in expansion.fields {
        let mut state = SplitState::AfterDelimiter;

        for piece in existing_field.0 {
            match piece {
                // An empty word contributes nothing (unlike a quoted empty string).
                ExpansionPiece::UnquotedLiteral(s) if s.is_empty() => {}
                // Quoted text and the word's own literal text go into the current field
                // untouched, opening one if needed. That is what keeps `""` alive as an
                // empty argument.
                ExpansionPiece::Unsplittable(_) | ExpansionPiece::UnquotedLiteral(_) => {
                    current_field.0.push(piece);
                    state = SplitState::InField;
                }
                ExpansionPiece::Splittable(s) => {
                    for c in s.chars() {
                        // An ordinary character extends the open field, or opens one.
                        // It joins the trailing splittable piece if there is one, so a
                        // field's text stays in as few pieces as possible.
                        if !ifs.contains(c) {
                            match current_field.0.last_mut() {
                                Some(ExpansionPiece::Splittable(last)) => last.push(c),
                                _ => current_field
                                    .0
                                    .push(ExpansionPiece::Splittable(c.to_string())),
                            }
                            state = SplitState::InField;
                            continue;
                        }

                        // A delimiter. What it does depends on whether it is soft
                        // (whitespace) or hard, and on where the last field ended.
                        state = match (state, is_ifs_whitespace(c)) {
                            // Either kind closes an open field. The kind decides whether
                            // the run that follows can still absorb a hard delimiter.
                            (SplitState::InField, whitespace) => {
                                fields.push(std::mem::take(&mut current_field));
                                if whitespace {
                                    SplitState::AfterWhitespace
                                } else {
                                    SplitState::AfterDelimiter
                                }
                            }
                            // The run's one hard delimiter: absorbed, nothing emitted.
                            (SplitState::AfterWhitespace, false) => SplitState::AfterDelimiter,
                            // A second hard delimiter, or one at the very start: it
                            // closes a field that never got any content.
                            (SplitState::AfterDelimiter, false) => {
                                fields.push(WordField::new());
                                SplitState::AfterDelimiter
                            }
                            // Whitespace between fields is skipped, whatever came
                            // before it; leading whitespace is dropped the same way.
                            (state, true) => state,
                        };
                    }
                }
            }
        }

        // Only an open field is left to flush. Ending on a delimiter adds nothing, so a
        // trailing `:` never yields a trailing empty field.
        if matches!(state, SplitState::InField) {
            fields.push(std::mem::take(&mut current_field));
        }
    }

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    type Pieces = Vec<Vec<ExpansionPiece>>;

    fn s(text: &str) -> ExpansionPiece {
        ExpansionPiece::Splittable(text.into())
    }

    fn u(text: &str) -> ExpansionPiece {
        ExpansionPiece::Unsplittable(text.into())
    }

    fn l(text: &str) -> ExpansionPiece {
        ExpansionPiece::UnquotedLiteral(text.into())
    }

    fn split(ifs: &str, fields: Pieces) -> Pieces {
        let expansion = Expansion {
            fields: fields.into_iter().map(WordField).collect(),
            ..Expansion::default()
        };
        split_fields(ifs, expansion)
            .into_iter()
            .map(|field| field.0)
            .collect()
    }

    #[test]
    fn splits_like_bash() {
        // (IFS, input fields, expected fields). Every expectation was checked against bash.
        let cases: Vec<(&str, Pieces, Pieces)> = vec![
            // Whitespace collapses and is dropped at either end.
            (
                " \t\n",
                vec![vec![s("  a \t b\n")]],
                vec![vec![s("a")], vec![s("b")]],
            ),
            // Non-whitespace delimiters keep the empty fields between them.
            (":", vec![vec![s("a:b")]], vec![vec![s("a")], vec![s("b")]]),
            (
                ":",
                vec![vec![s("a::b")]],
                vec![vec![s("a")], vec![], vec![s("b")]],
            ),
            // A leading delimiter leaves an empty field; a trailing one does not.
            (":", vec![vec![s(":a")]], vec![vec![], vec![s("a")]]),
            (":", vec![vec![s("a:")]], vec![vec![s("a")]]),
            (":", vec![vec![s(":")]], vec![vec![]]),
            (":", vec![vec![s("::")]], vec![vec![], vec![]]),
            // Whitespace next to a delimiter joins its run.
            (
                ": ",
                vec![vec![s("a : b")]],
                vec![vec![s("a")], vec![s("b")]],
            ),
            (
                ": ",
                vec![vec![s("a :: b")]],
                vec![vec![s("a")], vec![], vec![s("b")]],
            ),
            (": ", vec![vec![s(" :a")]], vec![vec![], vec![s("a")]]),
            (": ", vec![vec![s(": a")]], vec![vec![], vec![s("a")]]),
            (
                ": ",
                vec![vec![s(":  :a")]],
                vec![vec![], vec![], vec![s("a")]],
            ),
            (": ", vec![vec![s("a: ")]], vec![vec![s("a")]]),
            // The word's own literal text is never split; it joins whatever field is
            // open, and a delimiter that closed the previous field keeps it out.
            (":", vec![vec![l("a:b")]], vec![vec![l("a:b")]]),
            (
                ":",
                vec![vec![s("1:2"), l(":3")]],
                vec![vec![s("1")], vec![s("2"), l(":3")]],
            ),
            (
                ":",
                vec![vec![s("1:"), l(":3")]],
                vec![vec![s("1")], vec![l(":3")]],
            ),
            // Quoted text is never split and keeps its own piece.
            (
                ":",
                vec![vec![u("a:b"), s(":c")]],
                vec![vec![u("a:b")], vec![s("c")]],
            ),
            (
                ":",
                vec![vec![u("a:b"), s("c")]],
                vec![vec![u("a:b"), s("c")]],
            ),
            (":", vec![vec![u("")]], vec![vec![u("")]]),
            (
                ":",
                vec![vec![u("A")], vec![u("")]],
                vec![vec![u("A")], vec![u("")]],
            ),
            // A delimiter run may span two splittable pieces of one field...
            (
                ":",
                vec![vec![s("1:"), s(":4")]],
                vec![vec![s("1")], vec![], vec![s("4")]],
            ),
            // ...but never two incoming fields.
            (
                ":",
                vec![vec![s("a:b")], vec![s("c::d")]],
                vec![
                    vec![s("a")],
                    vec![s("b")],
                    vec![s("c")],
                    vec![],
                    vec![s("d")],
                ],
            ),
            // An empty unquoted expansion yields nothing.
            (":", vec![vec![s("")]], vec![]),
            // Empty IFS disables splitting.
            ("", vec![vec![s("a b")]], vec![vec![s("a b")]]),
            // The C-locale whitespace set is honored; Unicode spaces are ordinary delimiters.
            (
                "\r",
                vec![vec![s("a\rb\r\rc")]],
                vec![vec![s("a")], vec![s("b")], vec![s("c")]],
            ),
            (
                "\u{a0}",
                vec![vec![s("a\u{a0}\u{a0}b")]],
                vec![vec![s("a")], vec![], vec![s("b")]],
            ),
        ];

        for (ifs, input, expected) in cases {
            let actual = split(ifs, input.clone());
            assert_eq!(actual, expected, "ifs={ifs:?} input={input:?}");
        }
    }
}
