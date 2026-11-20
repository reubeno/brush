//! Reading the tokenizer's input with two chars of lookahead.

/// An iterator (of the chars of input) that can peek two items ahead: one more than
/// [`std::iter::Peekable`], to see whether a backslash starts a line continuation.
pub(super) struct CharReader<I: Iterator> {
    iter: std::iter::Peekable<I>,
    /// The next item, if taken from `iter` to peek past it (only ever briefly).
    taken: Option<I::Item>,
}

impl<I: Iterator> CharReader<I> {
    pub(super) fn new(iter: I) -> Self {
        Self {
            iter: iter.peekable(),
            taken: None,
        }
    }

    pub(super) fn peek(&mut self) -> Option<&I::Item> {
        match &self.taken {
            Some(item) => Some(item),
            None => self.iter.peek(),
        }
    }

    /// Returns the item after the next one, consuming neither.
    pub(super) fn peek_second(&mut self) -> Option<&I::Item> {
        if self.taken.is_none() {
            self.taken = Some(self.iter.next()?);
        }
        self.iter.peek()
    }
}

impl<I: Iterator> Iterator for CharReader<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        self.taken.take().or_else(|| self.iter.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peeks_up_to_two_ahead() {
        let mut reader = CharReader::new("ab".chars());
        assert_eq!(reader.peek_second(), Some(&'b'));
        assert_eq!(reader.peek(), Some(&'a'));
        assert_eq!(reader.next(), Some('a'));
        assert_eq!(reader.peek_second(), None);
        assert_eq!(reader.peek(), Some(&'b'));
        assert_eq!(reader.next(), Some('b'));
        assert_eq!(reader.peek(), None);
        assert_eq!(reader.peek_second(), None);
        assert_eq!(reader.next(), None);
    }
}
