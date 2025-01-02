/// 2.9.5 Function Definition Command
use winnow::{
    combinator::{cut_err, opt, preceded, repeat, separated_pair, trace},
    error::ErrMode,
    PResult, Parser as _,
};

use crate::{
    ast,
    parser2::{
        custom_combinators::non_posix_extension,
        io::redirect_list,
        trivia::{line_trailing, space, trim_whitespace},
        word::{self, space_after_word},
        Input,
    },
};

use super::{compound_command, insignificant};

pub fn function_definition<'i>(i: &mut Input<'i>) -> PResult<ast::FunctionDefinition> {
    trace("function_definition", move |i: &mut Input<'i>| {
        // N.B. Non-sh extensions allows use of the 'function' word to indicate a function
        let has_keyword = opt(non_posix_extension((
            "function",
            space_after_word,
            insignificant,
        )))
        .parse_next(i)?
        .is_some();
        // NOTE: there should be special rule for function identifier because
        // characters such as '$' | "'" | '"' are not allowed in Bash here. But it
        // unnesessary extra maintenance cost.
        // Maybe we should allow string here because any external command can be
        // named whatewer it wants, why not internal functions can be named `foo\;bar` ?
        let name_parser = move |i: &mut Input<'i>| {
            if has_keyword {
                // N.B if error occurs, stop parsing only if we have `function` keyword
                cut_err(word::non_empty(word::word)).parse_next(i)
            } else {
                word::non_empty(word::word).parse_next(i)
            }
        };
        let (name, body) = separated_pair(
            trim_whitespace(0.., name_parser, 0..),
            (
                "(",
                space(0..),
                cut_err(")"),
                repeat(0.., line_trailing).map(|()| ()),
                space(0..),
            ),
            cut_err(function_body),
        )
        .parse_next(i)?;
        Ok::<_, ErrMode<_>>((name, body))
    })
    .with_taken()
    .try_map(|((fname, body), source)| {
        let source = std::str::from_utf8(source)?;
        Ok::<_, std::str::Utf8Error>(ast::FunctionDefinition {
            fname: fname.into_owned(),
            body,
            source: source.to_string(),
        })
    })
    .parse_next(i)
}

fn function_body(i: &mut Input<'_>) -> PResult<ast::FunctionBody> {
    (
        compound_command::compound_command,
        (preceded(space(0..), opt(redirect_list))),
    )
        .map(|(c, r)| ast::FunctionBody(c, r))
        .parse_next(i)
}

#[cfg(test)]
mod tests {
    use crate::parser2::new_input;
    use crate::ParserOptions;

    use super::*;

    #[test]
    fn parse_function_definition() {
        fn parse<'i>(i: &'i str) {
            let io = function_definition
                .parse_next(&mut new_input(ParserOptions::default(), i))
                .unwrap();
            dbg!(io);
        }

        parse(
            r#"function



d@d1#ddd ()

{
    :
} 2>&1"#,
        )
    }
}
