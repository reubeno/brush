use std::{borrow::Cow, cell::RefCell, ops::RangeInclusive};

use std::str;
use winnow::combinator::fail;
use winnow::token::take_till;
use winnow::{
    combinator::{alt, cut_err, delimited, empty, not, opt, peek, preceded, repeat, trace},
    dispatch,
    error::ContextError,
    token::{any, one_of, take, take_while},
    PResult, Parser,
};

use crate::parser2::custom_combinators::non_posix_extension;

use super::custom_combinators::take_inside;
use super::{
    trivia::{line_trailing, space, ESCAPE, NON_ASCII, WHITESPACE_CHARS},
    Input,
};

// 2.4 Reserved Words
// https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html

// PERF: imroved lookup from using alt(RESERVED) where RESERVED = ["!", "for", ...] 164.60 ns/iter
// to 8.89 ns/iter. The compiler automatically optimizes the lookup based on the string length
fn reserved_word<'i>(i: &mut Input<'i>) -> PResult<()> {
    const WORD_SEP: &[u8] = &[
        b' ', b'\t', b'\r', b'\n', b'&', b'(', b')', b';', b'|', b'<', b'>',
    ];
    trace(
        "reserved_word",
        // TODO: set of token separators \n \t \r ' '
        dispatch! {take_till::<_, Input<'i>, _>(1.., WORD_SEP);
            b"!" | b"{" | b"}" | b"case" | b"do" | b"done" | b"elif" | b"else"
            | b"esac" | b"fi" | b"for" | b"if" | b"in" | b"then" | b"until" | b"while" => empty,
            b"[[" | b"]]" | b"function" | b"select"  => non_posix_extension(empty),
            _ => fail,
        },
    )
    .parse_next(i)
}
//
pub fn non_reserved<'i>(
    parser: impl Parser<Input<'i>, Cow<'i, str>, ContextError>,
) -> impl Parser<Input<'i>, Cow<'i, str>, ContextError> {
    trace("non_reserved_word", preceded(not(reserved_word), parser))
}

pub fn word<'i>(i: &mut Input<'i>) -> PResult<Cow<'i, str>> {
    trace(
        "word",
        dispatch! {peek(any);
            b'"' => double_quoted::double_quoted,
            b'\'' => single_quoted,
            _ => unquoted_string::unquoted_string,
        },
    )
    .parse_next(i)
}

pub(crate) fn space_after_word(i: &mut Input<'_>) -> PResult<()> {
    trace(
        "space_after_keyword",
        alt((line_trailing, cut_err(space(1..)))),
    )
    .void()
    .parse_next(i)
}

pub fn non_empty<'i>(
    parser: impl Parser<Input<'i>, Cow<'i, str>, ContextError>,
) -> impl Parser<Input<'i>, Cow<'i, str>, ContextError> {
    trace(
        "non_empty_string",
        parser.verify(|s: &Cow<'i, str>| !s.as_bytes().is_empty()),
    )
}

mod unquoted_string {
    use super::*;

    /// The first character is different from the rest ones, for example you cannot use
    /// an `#` as a first character, but it is possible to use it inside the word:
    ///     - `#echo` is a comment
    /// but - `ec#ho` is a string
    const FIRST_CHAR: (
        RangeInclusive<u8>,
        RangeInclusive<u8>,
        u8,
        RangeInclusive<u8>,
        RangeInclusive<u8>,
        RangeInclusive<u8>,
        RangeInclusive<u8>,
        RangeInclusive<u8>,
    ) = (
        // ascii hex codes
        // `$`, `%`
        0x24..=0x25,
        // `*` - `:`
        0x2A..=0x3A,
        // '='
        0x3D,
        // '?' - '['
        0x3F..=0x5B,
        // ']' - `_`
        0x5D..=0x5F,
        // 'a' - '{'
        0x61..=0x7B,
        // '}' - '~'
        0x7D..=0x7E,
        NON_ASCII,
    );

    // TODO: Im worried about that we forgetting what we escaped while parsing
    // what if the interpreter requires this information (probably according to Posix spec Im not
    // sure)
    // NOTE: word expansion is performed in word.rs expansion_parser

    fn transform_escaped<'i>(i: &mut Input<'i>) -> PResult<&'i str> {
        // TODO: list of special symbols
        let v = |v: &'static str| take(1u8).value(v);
        trace(
            "unquoted/transform_escaped",
            dispatch! {peek::<_, u8, _, _>(any);
                b'\\' => v(r"\"),
                b' ' => v(r" "),
                b'\n' => v(r""),
                b'"' => v(r#"""#),
                b';' => v(r";"),
                b'!' => v(r"!"),
                // N.B for the interpreter
                b'$' => v(r"\$"),
                // Do nothing.
                _ => empty.value(r"")
            },
        )
        .parse_next(i)
    }

    pub fn unquoted_string<'i>(i: &mut Input<'i>) -> PResult<Cow<'i, str>> {
        trace(
            "unquoted_string",
            non_empty(fold_escaped(
                // the first chunk until an escape
                trace(
                    "first_chunk",
                    opt((one_of(FIRST_CHAR), unquoted_chars(0..)))
                        .take()
                        .try_map(std::str::from_utf8),
                ),
                // decide what to do with the escaped character
                transform_escaped,
                // remaining chunks
                trace("chunk", unquoted_chars(0..)),
            )),
        )
        .parse_next(i)
    }

    // characters that are allowed inside the unquted string
    fn unquoted_chars<'i, R: Into<winnow::stream::Range>>(
        occurencies: R,
    ) -> impl Parser<Input<'i>, &'i str, ContextError> {
        // includes a single quote
        // basic-unescaped = wschar / %x21 / %x23-5B / %x5D-7E / non-ascii
        const UNQUOTED_CHARS: (
            RangeInclusive<u8>,
            RangeInclusive<u8>,
            u8,
            RangeInclusive<u8>,
            RangeInclusive<u8>,
            RangeInclusive<u8>,
            RangeInclusive<u8>,
            RangeInclusive<u8>,
        ) = (
            0x23..=0x25,
            0x2A..=0x3A,
            0x3D,
            0x3F..=0x5B,
            0x5D..=0x5F,
            0x61..=0x7B,
            0x7D..=0x7E,
            NON_ASCII,
        );
        trace("unquoted_chars", take_while(occurencies, UNQUOTED_CHARS))
            .try_map(std::str::from_utf8)
    }
}

// - `a s` -> `a` `s`
// - `a\ s` -> `a s`
// - `a\
//s` -> `as`
// quoted
// - `"a\ s"` -> `a\ s`
// - `a\
//s` -> `as`
pub fn single_quoted<'i>(i: &mut Input<'i>) -> PResult<Cow<'i, str>> {
    take_inside(b'\'', b'\'')
        .try_map(std::str::from_utf8)
        .map(Cow::Borrowed)
        .parse_next(i)
}

mod double_quoted {

    // https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html
    // The quote character sequence <dollar-sign> single-quote and the single-character quote
    // characters (<backslash>, single-quote, and double-quote) that were present in the original
    // word shall be removed unless they have themselves been quoted

    use super::*;

    pub(super) fn double_quoted<'i>(i: &mut Input<'i>) -> PResult<Cow<'i, str>> {
        trace(
            "double_quoted_string",
            delimited('"', string_body, cut_err('"')),
        )
        .parse_next(i)
    }

    // characters that are allowed inside the double quoted string
    fn unescaped_chars<'i, R: Into<winnow::stream::Range>>(
        occurencies: R,
    ) -> impl Parser<Input<'i>, &'i str, ContextError> {
        // includes a single quote
        // basic-unescaped = wschar / %x21 / %x23-5B / %x5D-7E / non-ascii
        const BASIC_UNESCAPED: (
            (u8, u8),
            u8,
            RangeInclusive<u8>,
            RangeInclusive<u8>,
            RangeInclusive<u8>,
        ) = (WHITESPACE_CHARS, 0x21, 0x23..=0x5B, 0x5D..=0x7E, NON_ASCII);
        trace(
            "double_quoted_unescaped_chars",
            take_while(occurencies, BASIC_UNESCAPED),
        )
        .try_map(std::str::from_utf8)
    }

    // special rules for escaping characters inside double quoted string
    fn transform_escaped<'i>(i: &mut Input<'i>) -> PResult<&'i str> {
        trace(
            "double_quoted_transform_escaped",
            dispatch! {peek::<_, u8, _, _>(any);
                b'\\' => take(1u8).value(r"\"),
                b'\n' => take(1u8).value(r""),
                b'"' => take(1u8).value(r#"""#),
                b'!' => take(1u8).value(r"\!"),
                // N.B for the interpreter
                b'$' => take(1u8).value(r"\$"),
                // Do nothing. Preserve `\`
                _ => empty.value(r"\")
            },
        )
        .parse_next(i)
    }

    fn string_body<'i>(i: &mut Input<'i>) -> PResult<Cow<'i, str>> {
        // escaped_transform(unescaped_chars(0..), '\\', transform_escaped)
        trace(
            "double_quoted_string_body",
            fold_escaped(
                unescaped_chars(0..),
                transform_escaped,
                unescaped_chars(0..),
            ),
        )
        .parse_next(i)
    }
}

fn fold_escaped<'i>(
    mut first_chunk: impl Parser<Input<'i>, &'i str, ContextError>,
    mut transform_escaped: impl Parser<Input<'i>, &'i str, ContextError> + 'i,
    mut chunk: impl Parser<Input<'i>, &'i str, ContextError>,
) -> impl Parser<Input<'i>, Cow<'i, str>, ContextError> {
    trace("fold_escaped", move |i: &mut Input<'i>| {
        // TODO: Sremove RefCell when fold(init, ) changes its api from FnMut to FnOnce
        // because SAFETY: this closure is called only once here.

        // the first chunk before the escape
        let res = RefCell::new(Cow::Borrowed(first_chunk.by_ref().parse_next(i)?));

        // process the escaped character and the next chunks
        repeat(
            0..,
            preceded(
                ESCAPE,
                (
                    // tramsform the escaped char, for example: consume '\n' from the stream but
                    // append nothing to the result ""
                    transform_escaped.by_ref(),
                    // the next chunk after escape
                    chunk.by_ref(),
                ),
            ),
        )
        .fold(
            || &res,
            |acc, (escaped, remaining)| {
                let mut c = acc.borrow_mut();
                match &mut *c {
                    Cow::Borrowed(lhs) => {
                        let mut s =
                            String::with_capacity(lhs.len() + escaped.len() + remaining.len());
                        s.push_str(lhs);
                        s.push_str(escaped);
                        s.push_str(remaining);
                        *c = Cow::Owned(s);
                    }
                    Cow::Owned(s) => {
                        s.reserve(escaped.len() + remaining.len());
                        s.push_str(escaped);
                        s.push_str(remaining);
                    }
                };
                acc
            },
        )
        .parse_next(i)?;
        Ok(res.take())
    })
}

#[cfg(test)]
mod tests {
    use crate::parser2::tests::input;

    use super::*;

    macro_rules! test_word {
        ($parser:ident ( $($case:ident ($i:literal) -> $expected:literal),+ $(,)?) ) => {
                $(
                    #[test]
                    fn $case() -> crate::parser2::tests::Result<()> {
                        assert_eq!($parser.parse(crate::parser2::new_input(crate::ParserOptions::default(), $i))?, $expected);
                        Ok(())
                    }
                )+

        };
    }
    mod test_double_quoted_string {
        use super::*;
        use crate::parser2::word::double_quoted::double_quoted;

        test_word! {double_quoted (
            empty(r#""""#) -> "",
            basic(r#""hello world""#) -> "hello world",
            escaped_quote(r#""a\"b""#) -> r#"a"b"#,
            escaped_from_start(r#""\"ab""#) -> r#""ab"#,
            escaped_newline(r#""\ \ \ \ \ \a\
        bc""#) -> r"\ \ \ \ \ \a        bc"

        )}
    }
    mod test_single_quoted_string {
        use super::*;

        test_word! {single_quoted (
            empty("''") -> "",
            basic("'hello world'") -> "hello world",
            double_quote_inside(r#"'a"b'"#) -> r#"a"b"#,
            escaped_newline(r#"'\ \ \ \ \ \a\
        bc'"#) -> r#"\ \ \ \ \ \a\
        bc"#
        )}
    }

    mod test_unquoted_string {
        use crate::parser2::new_input;
        use crate::ParserOptions;

        use super::*;
        use unquoted_string::unquoted_string;
        test_word! {unquoted_string (
            basic("ec#h1o") -> "ec#h1o",
            basic2("_echo") -> "_echo",
            basic3("4_echo") -> "4_echo",
            complicated(r#"-:1[zd*fa:a]]d1#2_-:@:"#) -> r#"-:1[zd*fa:a]]d1#2_-:@:"#,
            escaped_letter(r"\t") -> "t",
            escaped_keychar(r"a\!b") -> "a!b",
            escaped_keychar2(r"a\;b") -> "a;b",
            escaped_escape(r"ec\\ho") -> r"ec\ho",
            escaped_newline("\\ \\ \\ \\ \\ \\a\\\nbc") -> r#"     abc"#,
            escaped_newline2("a\\\n\\ ") -> r#"a "#
        )}

        #[test]
        fn test_empty_string() {
            assert_matches::assert_matches!(
                unquoted_string.parse(new_input(ParserOptions::default(), "")),
                Err(_)
            );
            assert_matches::assert_matches!(
                unquoted_string.parse(new_input(ParserOptions::default(), "\\\n")),
                Err(_)
            );
        }
    }

    extern crate test;
    use test::Bencher;

    #[test]
    fn test_reserved_keyword() {
        let mut input = input("while");
        let r = reserved_word.parse(input.clone()).unwrap();
        dbg!(r);
    }

    // #[bench]
    // fn bench_reserved_keyword(b: &mut Bencher) {
    //     let mut input = input("while");
    //     b.iter(|| {
    //         test::black_box(reserved_word.parse(input.clone()).unwrap());
    //     });
    // }
    // TODO: fix unquted word d\\d
}
