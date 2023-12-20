use anyhow::{Context, Result};

#[derive(Debug)]
pub enum WordPiece {
    Text(String),
    SingleQuotedText(String),
    DoubleQuotedSequence(Vec<WordPiece>),
    TildePrefix(String),
    ParameterExpansion(ParameterExpression),
    CommandSubstitution(String),
    EscapeSequence(String),
}

#[derive(Debug)]
pub enum ParameterTestType {
    UnsetOrNull,
    Unset,
}

#[derive(Debug)]
pub enum Parameter {
    Positional(u32),
    Special(SpecialParameter),
    Named(String),
}

#[derive(Debug)]
pub enum SpecialParameter {
    AllPositionalParameters { concatenate: bool },
    PositionalParameterCount,
    LastExitStatus,
    CurrentOptionFlags,
    ProcessId,
    LastBackgroundProcessId,
    ShellName,
}

#[derive(Debug)]
pub enum ParameterExpression {
    Parameter {
        parameter: Parameter,
    },
    UseDefaultValues {
        parameter: Parameter,
        test_type: ParameterTestType,
        default_value: Option<String>,
    },
    AssignDefaultValues {
        parameter: Parameter,
        test_type: ParameterTestType,
        default_value: Option<String>,
    },
    IndicateErrorIfNullOrUnset {
        parameter: Parameter,
        test_type: ParameterTestType,
        error_message: Option<String>,
    },
    UseAlternativeValue {
        parameter: Parameter,
        test_type: ParameterTestType,
        alternative_value: Option<String>,
    },
    StringLength {
        parameter: Parameter,
    },
    RemoveSmallestSuffixPattern {
        parameter: Parameter,
        pattern: Option<String>,
    },
    RemoveLargestSuffixPattern {
        parameter: Parameter,
        pattern: Option<String>,
    },
    RemoveSmallestPrefixPattern {
        parameter: Parameter,
        pattern: Option<String>,
    },
    RemoveLargestPrefixPattern {
        parameter: Parameter,
        pattern: Option<String>,
    },
}

pub fn parse_word_for_expansion(word: &str) -> Result<Vec<WordPiece>> {
    log::debug!("Parsing word '{}'", word);

    let pieces =
        expansion_parser::unexpanded_word(word).context(format!("parsing word '{}'", word))?;

    log::debug!("Parsed word '{}' => {{{:?}}}", word, pieces);

    Ok(pieces)
}

peg::parser! {
    grammar expansion_parser() for str {
        pub(crate) rule unexpanded_word() -> Vec<WordPiece> = word(<&[_]>)

        rule word<T>(stop_condition: rule<T>) -> Vec<WordPiece> =
            tilde:tilde_prefix()? pieces:word_piece(<stop_condition()>)* {
                let mut all_pieces = Vec::new();
                if let Some(tilde) = tilde {
                    all_pieces.push(tilde);
                }
                all_pieces.extend(pieces);
                all_pieces
            }

        // TODO: Handle quoting.
        rule word_piece<T>(stop_condition: rule<T>) -> WordPiece =
            command_substitution() /
            arithmetic_expansion() /
            parameter_expansion() /
            unquoted_text(<stop_condition()>)

        rule double_quoted_word_piece() -> WordPiece =
            command_substitution() /
            arithmetic_expansion() /
            parameter_expansion() /
            double_quoted_escape_sequence() /
            double_quoted_text()

        rule unquoted_text<T>(stop_condition: rule<T>) -> WordPiece =
            s:double_quoted_sequence() { WordPiece::DoubleQuotedSequence(s) } /
            s:single_quoted_literal_text() { WordPiece::SingleQuotedText(s.to_owned()) } /
            normal_escape_sequence() /
            unquoted_literal_text(<stop_condition()>)

        rule double_quoted_sequence() -> Vec<WordPiece> =
            "\"" i:double_quoted_sequence_inner()* "\"" { i }

        rule double_quoted_sequence_inner() -> WordPiece =
            double_quoted_word_piece()

        rule single_quoted_literal_text() -> &'input str =
            "\'" inner:$([^'\'']*) "\'" { inner }

        rule unquoted_literal_text<T>(stop_condition: rule<T>) -> WordPiece =
            s:$((stop_condition() !normal_escape_sequence() [^'$' | '\'' | '\"'])+) { WordPiece::Text(s.to_owned()) }

        rule double_quoted_text() -> WordPiece =
            s:double_quote_body_text() { WordPiece::Text(s.to_owned()) }

        rule double_quote_body_text() -> &'input str =
            $((!double_quoted_escape_sequence() [^'$' | '\"'])+)

        rule normal_escape_sequence() -> WordPiece =
            s:$("\\" [c]) { WordPiece::EscapeSequence(s.to_owned()) }

        rule double_quoted_escape_sequence() -> WordPiece =
            s:$("\\" ['$' | '`' | '\"' | '\'' | '\\']) { WordPiece::EscapeSequence(s.to_owned()) }

        // TODO: Handle colon syntax mentioned above
        rule tilde_prefix() -> WordPiece =
            "~" cs:$((!"/" [c])*) { WordPiece::TildePrefix(cs.to_owned()) }

        // TODO: Constrain syntax of parameter in brace-less form
        // TODO: Deal with fact that there may be a quoted word or escaped closing brace chars.
        // TODO: Improve on ow we handle a '$' not followed by a valid variable name or parameter.
        rule parameter_expansion() -> WordPiece =
            "${" e:parameter_expression() "}" {
                WordPiece::ParameterExpansion(e)
            } /
            "$" parameter:unbraced_parameter() {
                WordPiece::ParameterExpansion(ParameterExpression::Parameter { parameter })
            } /
            "$" {
                WordPiece::Text("$".to_owned())
            }

        rule parameter_expression() -> ParameterExpression =
            parameter:parameter() test_type:parameter_test_type() "-" default_value:parameter_expression_word()? {
                ParameterExpression::UseDefaultValues { parameter, test_type, default_value }
            } /
            parameter:parameter() test_type:parameter_test_type() "=" default_value:parameter_expression_word()? {
                ParameterExpression::AssignDefaultValues { parameter, test_type, default_value }
            } /
            parameter:parameter() test_type:parameter_test_type() "?" error_message:parameter_expression_word()? {
                ParameterExpression::IndicateErrorIfNullOrUnset { parameter, test_type, error_message }
            } /
            parameter:parameter() test_type:parameter_test_type() "+" alternative_value:parameter_expression_word()? {
                ParameterExpression::UseAlternativeValue { parameter, test_type, alternative_value }
            } /
            "#" parameter:parameter() {
                ParameterExpression::StringLength { parameter }
            } /
            parameter:parameter() "%" pattern:parameter_expression_word()? {
                ParameterExpression::RemoveSmallestSuffixPattern { parameter, pattern }
            } /
            parameter:parameter() "%%" pattern:parameter_expression_word()? {
                ParameterExpression::RemoveLargestSuffixPattern { parameter, pattern }
            } /
            parameter:parameter() "#" pattern:parameter_expression_word()? {
                ParameterExpression::RemoveSmallestPrefixPattern { parameter, pattern }
            } /
            parameter:parameter() "##" pattern:parameter_expression_word()? {
                ParameterExpression::RemoveLargestPrefixPattern { parameter, pattern }
            } /
            parameter:parameter() {
                ParameterExpression::Parameter { parameter }
            }

        rule parameter_test_type() -> ParameterTestType =
            colon:":"? {
                if colon.is_some() {
                    ParameterTestType::UnsetOrNull
                } else {
                    ParameterTestType::Unset
                }
            }

        rule unbraced_parameter() -> Parameter =
            p:unbraced_positional_parameter() { Parameter::Positional(p) } /
            p:special_parameter() { Parameter::Special(p) } /
            p:variable_name() { Parameter::Named(p.to_owned()) }

        rule parameter() -> Parameter =
            p:positional_parameter() { Parameter::Positional(p) } /
            p:special_parameter() { Parameter::Special(p) } /
            p:variable_name() { Parameter::Named(p.to_owned()) }

        rule positional_parameter() -> u32 =
            n:$(['1'..='9'](['0'..='9']*)) {? n.parse().or(Err("u32")) }
        rule unbraced_positional_parameter() -> u32 =
            n:$(['1'..='9']) {? n.parse().or(Err("u32")) }

        rule special_parameter() -> SpecialParameter =
            "@" { SpecialParameter::AllPositionalParameters { concatenate: false } } /
            "*" { SpecialParameter::AllPositionalParameters { concatenate: true } } /
            "#" { SpecialParameter::PositionalParameterCount } /
            "?" { SpecialParameter::LastExitStatus } /
            "-" { SpecialParameter::CurrentOptionFlags } /
            "$" { SpecialParameter::ProcessId } /
            "!" { SpecialParameter::LastBackgroundProcessId } /
            "0" { SpecialParameter::ShellName }

        rule variable_name() -> &'input str =
            $(!['0'..='9'] ['_' | '0'..='9' | 'a'..='z' | 'A'..='Z']+)

        rule command_substitution() -> WordPiece =
            "$(" c:command() ")" { WordPiece::CommandSubstitution(c.to_owned()) } /
            "`" backquoted_command() "`" { todo!("backquoted command substitution") }

        rule command() -> &'input str =
            $(command_piece() ** [' ' | '\t'])

        rule command_piece() -> WordPiece =
            word_piece(<![')']>)

        rule backquoted_command() -> () =
            "<BACKQUOTES UNIMPLEMENTED>" {}

        rule arithmetic_expansion() -> WordPiece =
            "$((" arithmetic_expression() "))" { todo!("arithmetic expression") }

        rule arithmetic_expression() -> () =
            "<ARITHMETIC EXPRESSIONS UNIMPLEMENTED>" {}

        rule parameter_expression_word() -> String =
            s:$(word(<!['}']>)) { s.to_owned() }
    }
}
