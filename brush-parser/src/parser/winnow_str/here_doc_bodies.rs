//! Where the bodies of the here-documents of one parse are
//!
//! A here-document's body does not follow its operator: it starts on the line
//! after the one the operator is on, and that line is parsed like any other
//! (`cat <<EOF | sort && echo ok`, `if cat <<EOF; then`). So the body is read
//! ahead when the operator is met, and the lines it occupies are stepped over
//! when the parser consumes the newline that ends the operator's line.
//!
//! The record is per parse and thread-local, because the newline parser is
//! reached from everywhere and carries no context.

use std::cell::RefCell;

/// The lines one here-document's body and closing delimiter occupy
#[derive(Clone, Copy)]
struct Claim {
    /// Offset of the operator, which identifies the here-document
    operator: usize,
    /// Offset of the first line after the operator's line
    after_line: usize,
    /// Offset just past the closing delimiter's line
    end: usize,
}

thread_local! {
    static PARSES: RefCell<Vec<Vec<Claim>>> = const { RefCell::new(Vec::new()) };
}

/// One parse's record, dropped with the parse
pub(super) struct Parse(());

impl Parse {
    pub(super) fn begin() -> Self {
        PARSES.with_borrow_mut(|parses| parses.push(Vec::new()));
        Self(())
    }
}

impl Drop for Parse {
    fn drop(&mut self) {
        PARSES.with_borrow_mut(Vec::pop);
    }
}

/// Where the body of the here-document at `operator` starts: the line after
/// the operator's, or after the bodies of the here-documents before it on
/// that line.
pub(super) fn body_start(operator: usize, after_line: usize) -> usize {
    PARSES.with_borrow(|parses| {
        let claims = parses.last().map(Vec::as_slice).unwrap_or_default();
        claims
            .iter()
            .filter(|c| c.after_line == after_line && c.operator < operator)
            .map(|c| c.end)
            .max()
            .unwrap_or(after_line)
    })
}

/// Record the lines the here-document at `operator` occupies. A branch the
/// parser abandons and takes again records the same lines again.
pub(super) fn claim(operator: usize, after_line: usize, end: usize) {
    PARSES.with_borrow_mut(|parses| {
        let Some(claims) = parses.last_mut() else {
            return;
        };
        claims.retain(|c| c.operator != operator);
        claims.push(Claim {
            operator,
            after_line,
            end,
        });
    });
}

/// The offset to continue at once the newline before `after_line` is
/// consumed, when here-document bodies start there
pub(super) fn end_of_bodies(after_line: usize) -> Option<usize> {
    PARSES.with_borrow(|parses| {
        let claims = parses.last()?;
        claims
            .iter()
            .filter(|c| c.after_line == after_line)
            .map(|c| c.end)
            .max()
    })
}
