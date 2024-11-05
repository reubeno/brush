use std::ops::RangeInclusive;

use winnow::{
    ascii::line_ending,
    combinator::{alt, delimited, eof, opt, trace},
    error::ParserError,
    prelude::*,
    stream::{AsChar, Stream, StreamIsPartial},
    token::take_while,
    PResult,
};

use super::Input;

pub(crate) const COMMENT: u8 = b'#';

pub(crate) const LF: u8 = b'\n';
pub(crate) const CR: u8 = b'\r';

// wschar = ( %x20 /              ; Space
//            %x09 )              ; Horizontal tab
pub(crate) const WHITESPACE_CHARS: (u8, u8) = (b' ', b'\t');

pub(crate) const ESCAPE: u8 = b'\\';

// non-ascii = %x80-D7FF / %xE000-10FFFF
// - ASCII is 0xxxxxxx
// - First byte for UTF-8 is 11xxxxxx
// - Subsequent UTF-8 bytes are 10xxxxxx
pub(crate) const NON_ASCII: RangeInclusive<u8> = 0x80..=0xff;

/// `non-eol = %x09 / %x20-7E / non-ascii`
pub(crate) const NON_EOL: (u8, RangeInclusive<u8>, RangeInclusive<u8>) =
    (0x09, 0x20..=0x7E, NON_ASCII);

/// `comment := # non-eol*`
pub fn comment(i: &mut Input<'_>) -> PResult<()> {
    (COMMENT, take_while(0.., NON_EOL)).void().parse_next(i)
}

// TODO: upstream into winnow because winnow has space0, space1
// and want to switch to ranges
pub fn space<Input, Error, R>(occurencies: R) -> impl Parser<Input, (), Error>
where
    R: Into<winnow::stream::Range>,
    Input: winnow::stream::StreamIsPartial + Stream,
    <Input as Stream>::Token: winnow::stream::AsChar + Clone,
    Error: ParserError<Input>,
{
    trace("space", take_while(occurencies, (' ', '\t')).void())
}

/// `line-space = whitespace* [comment]?`
pub fn line_space(i: &mut Input<'_>) -> PResult<()> {
    trace("line_space", (space(0..), opt(comment)))
        .void()
        .parse_next(i)
}

/// `line-trailing = line-space* line-ending`
pub(crate) fn line_trailing(i: &mut Input<'_>) -> PResult<()> {
    trace("line_trailing", (line_space, alt((line_ending, eof))))
        .void()
        .parse_next(i)
}

pub fn trim_whitespace<'i, Input, F, O, E, R1, R2>(
    occurencies1: R1,
    inner: F,
    occurencies2: R2,
) -> impl Parser<Input, O, E>
where
    R1: Into<winnow::stream::Range>,
    R2: Into<winnow::stream::Range>,
    Input: Stream + StreamIsPartial,
    <Input as Stream>::Token: AsChar + Clone,
    E: ParserError<Input>,
    F: Parser<Input, O, E>,
{
    trace(
        "trim_whitespace",
        delimited(space(occurencies1), inner, space(occurencies2)),
    )
}
