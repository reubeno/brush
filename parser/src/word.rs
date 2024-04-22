use crate::ast;
use crate::error;
use crate::ParserOptions;

#[derive(Debug)]
pub enum WordPiece {
    Text(String),
    SingleQuotedText(String),
    AnsiCQuotedText(String),
    DoubleQuotedSequence(Vec<WordPiece>),
    TildePrefix(String),
    ParameterExpansion(ParameterExpr),
    CommandSubstitution(String),
    EscapeSequence(String),
    ArithmeticExpression(ast::UnexpandedArithmeticExpr),
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
    NamedWithIndex { name: String, index: String },
    NamedWithAllIndices { name: String, concatenate: bool },
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
pub enum ParameterExpr {
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
    ParameterLength {
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
    Substring {
        parameter: Parameter,
        offset: ast::UnexpandedArithmeticExpr,
        length: Option<ast::UnexpandedArithmeticExpr>,
    },
    Transform {
        parameter: Parameter,
        op: ParameterTransformOp,
    },
    UppercaseFirstChar {
        parameter: Parameter,
        pattern: Option<String>,
    },
    UppercasePattern {
        parameter: Parameter,
        pattern: Option<String>,
    },
    LowercaseFirstChar {
        parameter: Parameter,
        pattern: Option<String>,
    },
    LowercasePattern {
        parameter: Parameter,
        pattern: Option<String>,
    },
    ReplaceSubstring {
        parameter: Parameter,
        pattern: String,
        replacement: String,
        match_kind: SubstringMatchKind,
    },
    VariableNames {
        prefix: String,
        concatenate: bool,
    },
    DereferenceVariable {
        variable_name: String,
    },
    MemberKeys {
        variable_name: String,
        concatenate: bool,
    },
}

#[derive(Debug)]
pub enum SubstringMatchKind {
    Prefix,
    Suffix,
    FirstOccurrence,
    Anywhere,
}

#[derive(Debug)]
pub enum ParameterTransformOp {
    CapitalizeInitial,
    ExpandEscapeSequences,
    PossiblyQuoteWithArraysExpanded { separate_words: bool },
    PromptExpand,
    Quoted,
    ToAssignmentLogic,
    ToAttributeFlags,
    ToLowerCase,
    ToUpperCase,
}

pub fn parse_word_for_expansion(
    word: &str,
    options: &ParserOptions,
) -> Result<Vec<WordPiece>, error::WordParseError> {
    log::debug!("Parsing word '{}'", word);

    let pieces = expansion_parser::unexpanded_word(word, options)
        .map_err(|err| error::WordParseError::Word(word.to_owned(), err))?;

    log::debug!("Parsed word '{}' => {{{:?}}}", word, pieces);

    Ok(pieces)
}

pub fn parse_parameter(
    word: &str,
    options: &ParserOptions,
) -> Result<Parameter, error::WordParseError> {
    let pieces = expansion_parser::parameter(word, options)
        .map_err(|err| error::WordParseError::Parameter(word.to_owned(), err))?;
    Ok(pieces)
}

peg::parser! {
    grammar expansion_parser(parser_options: &ParserOptions) for str {
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

        rule arithmetic_word<T>(stop_condition: rule<T>) -> Vec<WordPiece> =
            pieces:word_piece(<stop_condition()>)*

        rule word_piece<T>(stop_condition: rule<T>) -> WordPiece =
            arithmetic_expansion() /
            command_substitution() /
            parameter_expansion() /
            unquoted_text(<stop_condition()>)

        rule double_quoted_word_piece() -> WordPiece =
            arithmetic_expansion() /
            command_substitution() /
            parameter_expansion() /
            double_quoted_escape_sequence() /
            double_quoted_text()

        rule unquoted_text<T>(stop_condition: rule<T>) -> WordPiece =
            s:double_quoted_sequence() { WordPiece::DoubleQuotedSequence(s) } /
            s:single_quoted_literal_text() { WordPiece::SingleQuotedText(s.to_owned()) } /
            s:ansi_c_quoted_text() { WordPiece::AnsiCQuotedText(s.to_owned()) } /
            normal_escape_sequence() /
            unquoted_literal_text(<stop_condition()>)

        rule double_quoted_sequence() -> Vec<WordPiece> =
            "\"" i:double_quoted_sequence_inner()* "\"" { i }

        rule double_quoted_sequence_inner() -> WordPiece =
            double_quoted_word_piece()

        rule single_quoted_literal_text() -> &'input str =
            "\'" inner:$([^'\'']*) "\'" { inner }

        rule ansi_c_quoted_text() -> &'input str =
            "$\'" inner:$([^'\'']*) "\'" { inner }

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
            tilde_parsing_enabled() "~" cs:$((!"/" [c])*) { WordPiece::TildePrefix(cs.to_owned()) }

        // TODO: Constrain syntax of parameter in brace-less form
        // TODO: Deal with fact that there may be a quoted word or escaped closing brace chars.
        // TODO: Improve on how we handle a '$' not followed by a valid variable name or parameter.
        rule parameter_expansion() -> WordPiece =
            "${" e:parameter_expression() "}" {
                WordPiece::ParameterExpansion(e)
            } /
            "$" parameter:unbraced_parameter() {
                WordPiece::ParameterExpansion(ParameterExpr::Parameter { parameter })
            } /
            "$" !['\''] {
                WordPiece::Text("$".to_owned())
            }

        rule parameter_expression() -> ParameterExpr =
            parameter:parameter() test_type:parameter_test_type() "-" default_value:parameter_expression_word()? {
                ParameterExpr::UseDefaultValues { parameter, test_type, default_value }
            } /
            parameter:parameter() test_type:parameter_test_type() "=" default_value:parameter_expression_word()? {
                ParameterExpr::AssignDefaultValues { parameter, test_type, default_value }
            } /
            parameter:parameter() test_type:parameter_test_type() "?" error_message:parameter_expression_word()? {
                ParameterExpr::IndicateErrorIfNullOrUnset { parameter, test_type, error_message }
            } /
            parameter:parameter() test_type:parameter_test_type() "+" alternative_value:parameter_expression_word()? {
                ParameterExpr::UseAlternativeValue { parameter, test_type, alternative_value }
            } /
            "#" parameter:parameter() {
                ParameterExpr::ParameterLength { parameter }
            } /
            parameter:parameter() "%%" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveLargestSuffixPattern { parameter, pattern }
            } /
            parameter:parameter() "%" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveSmallestSuffixPattern { parameter, pattern }
            } /
            parameter:parameter() "##" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveLargestPrefixPattern { parameter, pattern }
            } /
            parameter:parameter() "#" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveSmallestPrefixPattern { parameter, pattern }
            } /
            // N.B. The following case is for non-sh extensions.
            non_posix_extensions_enabled() e:non_posix_parameter_expression() { e } /
            parameter:parameter() {
                ParameterExpr::Parameter { parameter }
            }

        rule parameter_test_type() -> ParameterTestType =
            colon:":"? {
                if colon.is_some() {
                    ParameterTestType::UnsetOrNull
                } else {
                    ParameterTestType::Unset
                }
            }

        rule non_posix_parameter_expression() -> ParameterExpr =
            "!" variable_name:variable_name() "[*]" {
                ParameterExpr::MemberKeys { variable_name: variable_name.to_owned(), concatenate: true }
            } /
            "!" variable_name:variable_name() "[@]" {
                ParameterExpr::MemberKeys { variable_name: variable_name.to_owned(), concatenate: false }
            } /
            "!" prefix:variable_name() "*" {
                ParameterExpr::VariableNames { prefix: prefix.to_owned(), concatenate: true }
            } /
            "!" prefix:variable_name() "@" {
                ParameterExpr::VariableNames { prefix: prefix.to_owned(), concatenate: false }
            } /
            "!" variable_name:variable_name() {
                ParameterExpr::DereferenceVariable { variable_name: variable_name.to_owned() }
            } /
            parameter:parameter() ":" offset:substring_offset() length:(":" l:substring_length() { l })? {
                ParameterExpr::Substring { parameter, offset, length }
            } /
            parameter:parameter() "@" op:non_posix_parameter_transformation_op() {
                ParameterExpr::Transform { parameter, op }
            } /
            parameter:parameter() "/#" pattern:parameter_search_pattern() "/" replacement:parameter_replacement_str() {
                ParameterExpr::ReplaceSubstring { parameter, pattern, replacement, match_kind: SubstringMatchKind::Prefix }
            } /
            parameter:parameter() "/%" pattern:parameter_search_pattern() "/" replacement:parameter_replacement_str() {
                ParameterExpr::ReplaceSubstring { parameter, pattern, replacement, match_kind: SubstringMatchKind::Suffix }
            } /
            parameter:parameter() "//" pattern:parameter_search_pattern() "/" replacement:parameter_replacement_str() {
                ParameterExpr::ReplaceSubstring { parameter, pattern, replacement, match_kind: SubstringMatchKind::Anywhere }
            } /
            parameter:parameter() "/" pattern:parameter_search_pattern() "/" replacement:parameter_replacement_str() {
                ParameterExpr::ReplaceSubstring { parameter, pattern, replacement, match_kind: SubstringMatchKind::FirstOccurrence }
            } /
            parameter:parameter() "^^" pattern:parameter_expression_word()? {
                ParameterExpr::UppercasePattern { parameter, pattern }
            } /
            parameter:parameter() "^" pattern:parameter_expression_word()? {
                ParameterExpr::UppercaseFirstChar { parameter, pattern }
            } /
            parameter:parameter() ",," pattern:parameter_expression_word()? {
                ParameterExpr::LowercasePattern { parameter, pattern }
            } /
            parameter:parameter() "," pattern:parameter_expression_word()? {
                ParameterExpr::LowercaseFirstChar { parameter, pattern }
            }

        rule non_posix_parameter_transformation_op() -> ParameterTransformOp =
            "U" { ParameterTransformOp::ToUpperCase } /
            "u" { ParameterTransformOp::CapitalizeInitial } /
            "L" { ParameterTransformOp::ToLowerCase } /
            "Q" { ParameterTransformOp::Quoted } /
            "E" { ParameterTransformOp::ExpandEscapeSequences } /
            "P" { ParameterTransformOp::PromptExpand } /
            "A" { ParameterTransformOp::ToAssignmentLogic } /
            "K" { ParameterTransformOp::PossiblyQuoteWithArraysExpanded { separate_words: false } } /
            "a" { ParameterTransformOp::ToAttributeFlags } /
            "k" { ParameterTransformOp::PossiblyQuoteWithArraysExpanded { separate_words: true } }


        rule unbraced_parameter() -> Parameter =
            p:unbraced_positional_parameter() { Parameter::Positional(p) } /
            p:special_parameter() { Parameter::Special(p) } /
            p:variable_name() { Parameter::Named(p.to_owned()) }

        // N.B. The indexing syntax is not a standard sh-ism.
        pub(crate) rule parameter() -> Parameter =
            p:positional_parameter() { Parameter::Positional(p) } /
            p:special_parameter() { Parameter::Special(p) } /
            non_posix_extensions_enabled() p:variable_name() "[@]" { Parameter::NamedWithAllIndices { name: p.to_owned(), concatenate: false } } /
            non_posix_extensions_enabled() p:variable_name() "[*]" { Parameter::NamedWithAllIndices { name: p.to_owned(), concatenate: true } } /
            non_posix_extensions_enabled() p:variable_name() "[" index:$((!"]" [_])*) "]" {?
                Ok(Parameter::NamedWithIndex { name: p.to_owned(), index: index.to_owned() })
            } /
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

        pub(crate) rule command_substitution() -> WordPiece =
            "$(" c:command() ")" { WordPiece::CommandSubstitution(c.to_owned()) } /
            "`" backquoted_command() "`" { todo!("UNIMPLEMENTED: backquoted command substitution") }

        pub(crate) rule command() -> &'input str =
            $(command_piece()*)

        pub(crate) rule command_piece() -> () =
            word_piece(<![')']>) {} /
            ([' ' | '\t'])+ {}

        rule backquoted_command() -> () =
            "<BACKQUOTES UNIMPLEMENTED>" {}

        rule arithmetic_expansion() -> WordPiece =
            "$((" e:$(arithmetic_word(<!"))">)) "))" { WordPiece::ArithmeticExpression(ast::UnexpandedArithmeticExpr { value: e.to_owned() } ) }

        rule substring_offset() -> ast::UnexpandedArithmeticExpr =
            s:$(arithmetic_word(<![':' | '}']>)) { ast::UnexpandedArithmeticExpr { value: s.to_owned() } }

        rule substring_length() -> ast::UnexpandedArithmeticExpr =
            s:$(arithmetic_word(<![':' | '}']>)) { ast::UnexpandedArithmeticExpr { value: s.to_owned() } }

        rule parameter_replacement_str() -> String =
            s:$(word(<!['}' | '/']>)) { s.to_owned() }

        rule parameter_search_pattern() -> String =
            s:$(word(<!['}' | '/']>)) { s.to_owned() }

        rule parameter_expression_word() -> String =
            s:$(word(<!['}']>)) { s.to_owned() }

        rule non_posix_extensions_enabled() -> () =
            &[_] {? if !parser_options.sh_mode { Ok(()) } else { Err("posix") } }

        rule tilde_parsing_enabled() -> () =
            &[_] {? if parser_options.tilde_expansion { Ok(()) } else { Err("no tilde expansion") } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use assert_matches::assert_matches;

    #[test]
    fn parse_command_substitution() -> Result<()> {
        super::expansion_parser::command_piece("echo", &ParserOptions::default())?;
        super::expansion_parser::command_piece("hi", &ParserOptions::default())?;
        super::expansion_parser::command("echo hi", &ParserOptions::default())?;
        super::expansion_parser::command_substitution("$(echo hi)", &ParserOptions::default())?;

        let parsed = super::parse_word_for_expansion("$(echo hi)", &ParserOptions::default())?;
        assert_matches!(
            &parsed[..],
            [WordPiece::CommandSubstitution(s)] if s.as_str() == "echo hi"
        );

        Ok(())
    }

    #[test]
    fn parse_command_substitution_with_embedded_quotes() -> Result<()> {
        super::expansion_parser::command_piece("echo", &ParserOptions::default())?;
        super::expansion_parser::command_piece(r#""hi""#, &ParserOptions::default())?;
        super::expansion_parser::command(r#"echo "hi""#, &ParserOptions::default())?;
        super::expansion_parser::command_substitution(
            r#"$(echo "hi")"#,
            &ParserOptions::default(),
        )?;

        let parsed = super::parse_word_for_expansion(r#"$(echo "hi")"#, &ParserOptions::default())?;
        assert_matches!(
            &parsed[..],
            [WordPiece::CommandSubstitution(s)] if s.as_str() == r#"echo "hi""#
        );

        Ok(())
    }
}
