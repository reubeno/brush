use anyhow::Result;

pub fn pattern_to_regex_str(pattern: &str) -> Result<String> {
    let regex_str = pattern_to_regex_translator::pattern(pattern)?;
    Ok(regex_str)
}

peg::parser! {
    grammar pattern_to_regex_translator() for str {
        pub(crate) rule pattern() -> String =
            pieces:(pattern_piece()*) {
                pieces.join("")
            }

        rule pattern_piece() -> String =
            escape_sequence() /
            bracket_expression() /
            wildcard() /
            c:[_] { c.to_string() }

        rule escape_sequence() -> String =
            "\\" c:[_] { c.to_string() }

        rule bracket_expression() -> String =
            "[" invert:(("!")?) members:bracket_member()+ "]" {
                let mut members = members;
                if invert.is_some() {
                    members.insert(0, String::from("^"));
                }
                members.join("")
            }

        rule bracket_member() -> String =
            char_class_expression() /
            char_range()

        rule char_class_expression() -> String =
            e:$("[:" char_class() ":]") { e.to_owned() }

        rule char_class() =
            "alnum" / "alpha" / "blank" / "cntrl" / "digit" / "graph" / "lower" / "print" / "punct" / "space" / "upper"/ "xdigit"

        rule char_range() -> String =
            range:$([_] "-" [_]) { range.to_owned() }

        rule wildcard() -> String =
            "?" { String::from(".") } /
            "*" { String::from(".*") }
    }
}
