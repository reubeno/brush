//! Parser for shell words, used in expansion and other contexts.
//!
//! Implements support for:
//!
//! - Text quoting (single, double, ANSI C).
//! - Escape sequences.
//! - Tilde prefixes.
//! - Parameter expansion expressions.
//! - Command substitution expressions.
//! - Arithmetic expansion expressions.

use crate::ast;
use crate::error;
use crate::ParserOptions;

/// Alias for string type used with words.
pub type WordString = imstr::ImString;

/// Encapsulates a `WordPiece` together with its position in the string it came from.
#[derive(Clone, Debug)]
pub struct WordPieceWithSource {
    /// The word piece.
    pub piece: WordPiece,
    /// The start index of the piece in the source string.
    pub start_index: usize,
    /// The end index of the piece in the source string.
    pub end_index: usize,
}

/// Represents a piece of a word.
#[derive(Clone, Debug)]
pub enum WordPiece {
    /// A simple unquoted, unescaped string.
    Text(WordString),
    /// A string that is single-quoted.
    SingleQuotedText(WordString),
    /// A string that is ANSI-C quoted.
    AnsiCQuotedText(WordString),
    /// A sequence of pieces that are embedded in double quotes.
    DoubleQuotedSequence(Vec<WordPieceWithSource>),
    /// A tilde prefix.
    TildePrefix(WordString),
    /// A parameter expansion.
    ParameterExpansion(ParameterExpr),
    /// A command substitution.
    CommandSubstitution(WordString),
    /// A backquoted command substitution.
    BackquotedCommandSubstitution(WordString),
    /// An escape sequence.
    EscapeSequence(WordString),
    /// An arithmetic expression.
    ArithmeticExpression(ast::UnexpandedArithmeticExpr),
}

/// Type of a parameter test.
#[derive(Clone, Debug)]
pub enum ParameterTestType {
    /// Check for unset or null.
    UnsetOrNull,
    /// Check for unset.
    Unset,
}

/// A parameter, used in a parameter expansion.
#[derive(Clone, Debug)]
pub enum Parameter {
    /// A 0-indexed positional parameter.
    Positional(u32),
    /// A special parameter.
    Special(SpecialParameter),
    /// A named variable.
    Named(WordString),
    /// An index into a named variable.
    NamedWithIndex {
        /// Variable name.
        name: WordString,
        /// Index.
        index: WordString,
    },
    /// A named array variable with all indices.
    NamedWithAllIndices {
        /// Variable name.
        name: WordString,
        /// Whether to concatenate the values.
        concatenate: bool,
    },
}

/// A special parameter, used in a parameter expansion.
#[derive(Clone, Debug)]
pub enum SpecialParameter {
    /// All positional parameters.
    AllPositionalParameters {
        /// Whether to concatenate the values.
        concatenate: bool,
    },
    /// The count of positional parameters.
    PositionalParameterCount,
    /// The last exit status in the shell.
    LastExitStatus,
    /// The current shell option flags.
    CurrentOptionFlags,
    /// The current shell process ID.
    ProcessId,
    /// The last background process ID managed by the shell.
    LastBackgroundProcessId,
    /// The name of the shell.
    ShellName,
}

/// A parameter expression, used in a parameter expansion.
#[derive(Clone, Debug)]
pub enum ParameterExpr {
    /// A parameter, with optional indirection.
    Parameter {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
    },
    /// Conditionally use default values.
    UseDefaultValues {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// The type of test to perform.
        test_type: ParameterTestType,
        /// Default value to conditionally use.
        default_value: Option<WordString>,
    },
    /// Conditionally assign default values.
    AssignDefaultValues {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// The type of test to perform.
        test_type: ParameterTestType,
        /// Default value to conditionally assign.
        default_value: Option<WordString>,
    },
    /// Indicate error if null or unset.
    IndicateErrorIfNullOrUnset {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// The type of test to perform.
        test_type: ParameterTestType,
        /// Error message to conditionally yield.
        error_message: Option<WordString>,
    },
    /// Conditionally use an alternative value.
    UseAlternativeValue {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// The type of test to perform.
        test_type: ParameterTestType,
        /// Alternative value to conditionally use.
        alternative_value: Option<WordString>,
    },
    /// Compute the length of the given parameter.
    ParameterLength {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
    },
    /// Remove the smallest suffix from the given string matching the given pattern.
    RemoveSmallestSuffixPattern {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Remove the largest suffix from the given string matching the given pattern.
    RemoveLargestSuffixPattern {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Remove the smallest prefix from the given string matching the given pattern.
    RemoveSmallestPrefixPattern {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Remove the largest prefix from the given string matching the given pattern.
    RemoveLargestPrefixPattern {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Extract a substring from the given parameter.
    Substring {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Arithmetic expression that will be expanded to compute the offset
        /// at which the substring should be extracted.
        offset: ast::UnexpandedArithmeticExpr,
        /// Optionally provides an arithmetic expression that will be expanded
        /// to compute the length of substring to be extracted; if left
        /// unspecified, the remainder of the string will be extracted.
        length: Option<ast::UnexpandedArithmeticExpr>,
    },
    /// Transform the given parameter.
    Transform {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Type of transformation to apply.
        op: ParameterTransformOp,
    },
    /// Uppercase the first character of the given parameter.
    UppercaseFirstChar {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Uppercase the portion of the given parameter matching the given pattern.
    UppercasePattern {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Lowercase the first character of the given parameter.
    LowercaseFirstChar {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Lowercase the portion of the given parameter matching the given pattern.
    LowercasePattern {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Optionally provides a pattern to match.
        pattern: Option<WordString>,
    },
    /// Replace occurrences of the given pattern in the given parameter.
    ReplaceSubstring {
        /// The parameter.
        parameter: Parameter,
        /// Whether to treat the expanded parameter as an indirect
        /// reference, which should be subsequently dereferenced
        /// for the expansion.
        indirect: bool,
        /// Pattern to match.
        pattern: WordString,
        /// Replacement string.
        replacement: Option<WordString>,
        /// Kind of match to perform.
        match_kind: SubstringMatchKind,
    },
    /// Select variable names from the environment with a given prefix.
    VariableNames {
        /// The prefix to match.
        prefix: WordString,
        /// Whether to concatenate the results.
        concatenate: bool,
    },
    /// Select member keys from the named array.
    MemberKeys {
        /// Name of the array variable.
        variable_name: WordString,
        /// Whether to concatenate the results.
        concatenate: bool,
    },
}

/// Kind of substring match.
#[derive(Clone, Debug)]
pub enum SubstringMatchKind {
    /// Match the prefix of the string.
    Prefix,
    /// Match the suffix of the string.
    Suffix,
    /// Match the first occurrence in the string.
    FirstOccurrence,
    /// Match all instances in the string.
    Anywhere,
}

/// Kind of operation to apply to a parameter.
#[derive(Clone, Debug)]
pub enum ParameterTransformOp {
    /// Capitalizate initials.
    CapitalizeInitial,
    /// Expand escape sequences.
    ExpandEscapeSequences,
    /// Possibly quote with arrays expanded.
    PossiblyQuoteWithArraysExpanded {
        /// Whether or not to yield separate words.
        separate_words: bool,
    },
    /// Apply prompt expansion.
    PromptExpand,
    /// Quote the parameter.
    Quoted,
    /// Translate to a format usable in an assignment/declaration.
    ToAssignmentLogic,
    /// Translate to the parameter's attribute flags.
    ToAttributeFlags,
    /// Translate to lowercase.
    ToLowerCase,
    /// Translate to uppercase.
    ToUpperCase,
}

/// Represents a sub-word that is either a brace expression or some other word text.
#[derive(Clone, Debug)]
pub enum BraceExpressionOrText {
    /// A brace expression.
    Expr(BraceExpression),
    /// Other word text.
    Text(WordString),
}

/// Represents a brace expression to be expanded.
pub type BraceExpression = Vec<BraceExpressionMember>;

impl BraceExpressionOrText {
    /// Generates expansions for this value.
    pub fn generate(self) -> Box<dyn Iterator<Item = WordString>> {
        match self {
            BraceExpressionOrText::Expr(members) => {
                let mut iters = vec![];
                for m in members {
                    iters.push(m.generate());
                }
                Box::new(iters.into_iter().flatten())
            }
            BraceExpressionOrText::Text(text) => Box::new(std::iter::once(text)),
        }
    }
}

/// Member of a brace expression.
#[derive(Clone, Debug)]
pub enum BraceExpressionMember {
    /// An inclusive numerical sequence.
    NumberSequence {
        /// Start of the sequence.
        low: i64,
        /// Inclusive end of the sequence.
        high: i64,
        /// Increment value.
        increment: i64,
    },
    /// An inclusive character sequence.
    CharSequence {
        /// Start of the sequence.
        low: char,
        /// Inclusive end of the sequence.
        high: char,
        /// Increment value.
        increment: i64,
    },
    /// Text.
    Text(WordString),
}

impl BraceExpressionMember {
    /// Generates expansions for this member.
    pub fn generate(self) -> Box<dyn Iterator<Item = WordString>> {
        match self {
            BraceExpressionMember::NumberSequence {
                low,
                high,
                increment,
            } => Box::new(
                (low..=high)
                    .step_by(increment as usize)
                    .map(|n| n.to_string().into()),
            ),
            BraceExpressionMember::CharSequence {
                low,
                high,
                increment,
            } => Box::new((low..=high).step_by(increment as usize).map(|c| c.into())),
            BraceExpressionMember::Text(text) => Box::new(std::iter::once(text)),
        }
    }
}

/// Parse a word into its constituent pieces.
///
/// # Arguments
///
/// * `word` - The word to parse.
/// * `options` - The parser options to use.
pub fn parse(
    word: &WordString,
    options: &ParserOptions,
) -> Result<Vec<WordPieceWithSource>, error::WordParseError> {
    cacheable_parse(word.clone(), options.to_owned())
}

#[cached::proc_macro::cached(size = 64, result = true)]
fn cacheable_parse(
    word: WordString,
    options: ParserOptions,
) -> Result<Vec<WordPieceWithSource>, error::WordParseError> {
    tracing::debug!(target: "expansion", "Parsing word '{}'", word);

    let pieces = expansion_parser::unexpanded_word(&word, &options)
        .map_err(|err| error::WordParseError::Word(word.clone().into(), err))?;

    tracing::debug!(target: "expansion", "Parsed word '{}' => {{{:?}}}", word.as_str(), pieces);

    Ok(pieces)
}

/// Parse the given word into a parameter expression.
///
/// # Arguments
///
/// * `word` - The word to parse.
/// * `options` - The parser options to use.
pub fn parse_parameter(
    word: &WordString,
    options: &ParserOptions,
) -> Result<Parameter, error::WordParseError> {
    expansion_parser::parameter(word, options)
        .map_err(|err| error::WordParseError::Parameter(word.clone(), err))
}

/// Parse brace expansion from a given word .
///
/// # Arguments
///
/// * `word` - The word to parse.
/// * `options` - The parser options to use.
pub fn parse_brace_expansions(
    word: &WordString,
    options: &ParserOptions,
) -> Result<Option<Vec<BraceExpressionOrText>>, error::WordParseError> {
    expansion_parser::brace_expansions(word, options)
        .map_err(|err| error::WordParseError::BraceExpansion(word.clone(), err))
}

peg::parser! {
    grammar expansion_parser(parser_options: &ParserOptions) for WordString {
        pub(crate) rule unexpanded_word() -> Vec<WordPieceWithSource> = word(<![_]>)

        rule word<T>(stop_condition: rule<T>) -> Vec<WordPieceWithSource> =
            tilde:tilde_prefix_with_source()? pieces:word_piece_with_source(<stop_condition()>, false /*in_command*/)* {
                let mut all_pieces = Vec::new();
                if let Some(tilde) = tilde {
                    all_pieces.push(tilde);
                }
                all_pieces.extend(pieces);
                all_pieces
            }

        pub(crate) rule brace_expansions() -> Option<Vec<BraceExpressionOrText>> =
            pieces:(brace_expansion_piece()+) { Some(pieces) } /
            [_]* { None }

        rule brace_expansion_piece() -> BraceExpressionOrText =
            expr:brace_expr() {
                BraceExpressionOrText::Expr(expr)
            } /
            text:$(non_brace_expr_text()+) { BraceExpressionOrText::Text(text) }

        rule non_brace_expr_text() -> () =
            !"{" word_piece(<['{']>, false) {} /
            !brace_expr() "{" {}

        rule brace_expr() -> BraceExpression =
            "{" inner:brace_expr_inner() "}" { inner }

        rule brace_expr_inner() -> BraceExpression =
            brace_text_list_expr() /
            seq:brace_sequence_expr() { vec![seq] }

        rule brace_text_list_expr() -> BraceExpression =
            members:(brace_text_list_member() **<2,> ",") {
                members.into_iter().flatten().collect()
            }

        rule brace_text_list_member() -> BraceExpression =
            &[',' | '}'] { vec![BraceExpressionMember::Text(WordString::new())] } /
            brace_expr() /
            text:$(word_piece(<[',' | '}']>, false)) {
                vec![BraceExpressionMember::Text(text)]
            }

        rule brace_sequence_expr() -> BraceExpressionMember =
            low:number() ".." high:number() increment:(".." n:number() { n })? {
                BraceExpressionMember::NumberSequence { low, high, increment: increment.unwrap_or(1) }
            } /
            low:character() ".." high:character() increment:(".." n:number() { n })? {
                BraceExpressionMember::CharSequence { low, high, increment: increment.unwrap_or(1) }
            }

        rule number() -> i64 = n:$(['0'..='9']+) { n.parse().unwrap() }
        rule character() -> char = ['a'..='z' | 'A'..='Z']

        // N.B. We don't bother returning the word pieces, as all users of this rule
        // only try to extract the consumed input string and not the parse result.
        rule arithmetic_word<T>(stop_condition: rule<T>) =
            arithmetic_word_piece(<stop_condition()>)* {}

        rule arithmetic_word_piece<T>(stop_condition: rule<T>) =
            "(" arithmetic_word_plus_right_paren() {} /
            word_piece(<param_rule_or_open_paren(<stop_condition()>)>, false /*in_command*/) {}

        rule param_rule_or_open_paren<T>(stop_condition: rule<T>) -> () =
            stop_condition() {} /
            "(" {}

        rule arithmetic_word_plus_right_paren() =
            "(" arithmetic_word_plus_right_paren() ")" /
            word_piece(<[')']>, false /*in_command*/)* ")" {}

        rule word_piece_with_source<T>(stop_condition: rule<T>, in_command: bool) -> WordPieceWithSource =
            start_index:position!() piece:word_piece(<stop_condition()>, in_command) end_index:position!() {
                WordPieceWithSource { piece, start_index, end_index }
            }

        rule word_piece<T>(stop_condition: rule<T>, in_command: bool) -> WordPiece =
            arithmetic_expansion() /
            command_substitution() /
            parameter_expansion() /
            unquoted_text(<stop_condition()>, in_command)

        rule double_quoted_word_piece() -> WordPiece =
            arithmetic_expansion() /
            command_substitution() /
            parameter_expansion() /
            double_quoted_escape_sequence() /
            double_quoted_text()

        rule unquoted_text<T>(stop_condition: rule<T>, in_command: bool) -> WordPiece =
            s:double_quoted_sequence() { WordPiece::DoubleQuotedSequence(s) } /
            s:single_quoted_literal_text() { WordPiece::SingleQuotedText(s) } /
            s:ansi_c_quoted_text() { WordPiece::AnsiCQuotedText(s) } /
            normal_escape_sequence() /
            unquoted_literal_text(<stop_condition()>, in_command)

        rule double_quoted_sequence() -> Vec<WordPieceWithSource> =
            "\"" i:double_quoted_sequence_inner()* "\"" { i }

        rule double_quoted_sequence_inner() -> WordPieceWithSource =
            start_index:position!() piece:double_quoted_word_piece() end_index:position!() {
                WordPieceWithSource {
                    piece,
                    start_index,
                    end_index
                }
            }

        rule single_quoted_literal_text() -> WordString =
            "\'" inner:$([^'\'']*) "\'" { inner }

        rule ansi_c_quoted_text() -> WordString =
            "$\'" inner:$([^'\'']*) "\'" { inner }

        rule unquoted_literal_text<T>(stop_condition: rule<T>, in_command: bool) -> WordPiece =
            s:$(unquoted_literal_text_piece(<stop_condition()>, in_command)+) { WordPiece::Text(s) }

        // TODO: Find a way to remove the special-case logic for extglob + subshell commands
        rule unquoted_literal_text_piece<T>(stop_condition: rule<T>, in_command: bool) =
            is_true(in_command) extglob_pattern() /
            is_true(in_command) subshell_command() /
            !stop_condition() !normal_escape_sequence() [^'$' | '\'' | '\"'] {}

        rule is_true(value: bool) = &[_] {? if value { Ok(()) } else { Err("not true") } }

        rule extglob_pattern() =
            ("@" / "!" / "?" / "+" / "*") "(" extglob_body_piece()* ")" {}

        rule extglob_body_piece() =
            word_piece(<[')']>, true /*in_command*/) {}

        rule subshell_command() =
            "(" command() ")" {}

        rule double_quoted_text() -> WordPiece =
            s:double_quote_body_text() { WordPiece::Text(s) }

        rule double_quote_body_text() -> WordString =
            $((!double_quoted_escape_sequence() [^'$' | '\"'])+)

        rule normal_escape_sequence() -> WordPiece =
            s:$("\\" [c]) { WordPiece::EscapeSequence(s) }

        rule double_quoted_escape_sequence() -> WordPiece =
            s:$("\\" ['$' | '`' | '\"' | '\'' | '\\']) { WordPiece::EscapeSequence(s) }

        rule tilde_prefix_with_source() -> WordPieceWithSource =
            start_index:position!() piece:tilde_prefix() end_index:position!() {
                WordPieceWithSource {
                    piece,
                    start_index,
                    end_index
                }
            }

        // TODO: Handle colon syntax
        rule tilde_prefix() -> WordPiece =
            tilde_parsing_enabled() "~" cs:$((!['/' | ':' | ';'] [c])*) { WordPiece::TildePrefix(cs) }

        // TODO: Deal with fact that there may be a quoted word or escaped closing brace chars.
        // TODO: Improve on how we handle a '$' not followed by a valid variable name or parameter.
        rule parameter_expansion() -> WordPiece =
            "${" e:parameter_expression() "}" {
                WordPiece::ParameterExpansion(e)
            } /
            "$" parameter:unbraced_parameter() {
                WordPiece::ParameterExpansion(ParameterExpr::Parameter { parameter, indirect: false })
            } /
            "$" !['\''] {
                WordPiece::Text("$".into())
            }

        rule parameter_expression() -> ParameterExpr =
            indirect:parameter_indirection() parameter:parameter() test_type:parameter_test_type() "-" default_value:parameter_expression_word()? {
                ParameterExpr::UseDefaultValues { parameter, indirect, test_type, default_value }
            } /
            indirect:parameter_indirection() parameter:parameter() test_type:parameter_test_type() "=" default_value:parameter_expression_word()? {
                ParameterExpr::AssignDefaultValues { parameter, indirect, test_type, default_value }
            } /
            indirect:parameter_indirection() parameter:parameter() test_type:parameter_test_type() "?" error_message:parameter_expression_word()? {
                ParameterExpr::IndicateErrorIfNullOrUnset { parameter, indirect, test_type, error_message }
            } /
            indirect:parameter_indirection() parameter:parameter() test_type:parameter_test_type() "+" alternative_value:parameter_expression_word()? {
                ParameterExpr::UseAlternativeValue { parameter, indirect, test_type, alternative_value }
            } /
            "#" parameter:parameter() {
                ParameterExpr::ParameterLength { parameter, indirect: false }
            } /
            indirect:parameter_indirection() parameter:parameter() "%%" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveLargestSuffixPattern { parameter, indirect, pattern }
            } /
            indirect:parameter_indirection() parameter:parameter() "%" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveSmallestSuffixPattern { parameter, indirect, pattern }
            } /
            indirect:parameter_indirection() parameter:parameter() "##" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveLargestPrefixPattern { parameter, indirect, pattern }
            } /
            indirect:parameter_indirection() parameter:parameter() "#" pattern:parameter_expression_word()? {
                ParameterExpr::RemoveSmallestPrefixPattern { parameter, indirect, pattern }
            } /
            // N.B. The following case is for non-sh extensions.
            non_posix_extensions_enabled() e:non_posix_parameter_expression() { e } /
            indirect:parameter_indirection() parameter:parameter() {
                ParameterExpr::Parameter { parameter, indirect }
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
                ParameterExpr::MemberKeys { variable_name, concatenate: true }
            } /
            "!" variable_name:variable_name() "[@]" {
                ParameterExpr::MemberKeys { variable_name, concatenate: false }
            } /
            "!" prefix:variable_name() "*" {
                ParameterExpr::VariableNames { prefix, concatenate: true }
            } /
            "!" prefix:variable_name() "@" {
                ParameterExpr::VariableNames { prefix, concatenate: false }
            } /
            indirect:parameter_indirection() parameter:parameter() ":" offset:substring_offset() length:(":" l:substring_length() { l })? {
                ParameterExpr::Substring { parameter, indirect, offset, length }
            } /
            indirect:parameter_indirection() parameter:parameter() "@" op:non_posix_parameter_transformation_op() {
                ParameterExpr::Transform { parameter, indirect, op }
            } /
            indirect:parameter_indirection() parameter:parameter() "/#" pattern:parameter_search_pattern() replacement:parameter_replacement_str()? {
                ParameterExpr::ReplaceSubstring { parameter, indirect, pattern, replacement, match_kind: SubstringMatchKind::Prefix }
            } /
            indirect:parameter_indirection() parameter:parameter() "/%" pattern:parameter_search_pattern() replacement:parameter_replacement_str()? {
                ParameterExpr::ReplaceSubstring { parameter, indirect, pattern, replacement, match_kind: SubstringMatchKind::Suffix }
            } /
            indirect:parameter_indirection() parameter:parameter() "//" pattern:parameter_search_pattern() replacement:parameter_replacement_str()? {
                ParameterExpr::ReplaceSubstring { parameter, indirect, pattern, replacement, match_kind: SubstringMatchKind::Anywhere }
            } /
            indirect:parameter_indirection() parameter:parameter() "/" pattern:parameter_search_pattern() replacement:parameter_replacement_str()? {
                ParameterExpr::ReplaceSubstring { parameter, indirect, pattern, replacement, match_kind: SubstringMatchKind::FirstOccurrence }
            } /
            indirect:parameter_indirection() parameter:parameter() "^^" pattern:parameter_expression_word()? {
                ParameterExpr::UppercasePattern { parameter, indirect, pattern }
            } /
            indirect:parameter_indirection() parameter:parameter() "^" pattern:parameter_expression_word()? {
                ParameterExpr::UppercaseFirstChar { parameter, indirect, pattern }
            } /
            indirect:parameter_indirection() parameter:parameter() ",," pattern:parameter_expression_word()? {
                ParameterExpr::LowercasePattern { parameter, indirect, pattern }
            } /
            indirect:parameter_indirection() parameter:parameter() "," pattern:parameter_expression_word()? {
                ParameterExpr::LowercaseFirstChar { parameter, indirect, pattern }
            }

        rule parameter_indirection() -> bool =
            non_posix_extensions_enabled() "!" { true } /
            { false }

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
            p:variable_name() { Parameter::Named(p) }

        // N.B. The indexing syntax is not a standard sh-ism.
        pub(crate) rule parameter() -> Parameter =
            p:positional_parameter() { Parameter::Positional(p) } /
            p:special_parameter() { Parameter::Special(p) } /
            non_posix_extensions_enabled() p:variable_name() "[@]" { Parameter::NamedWithAllIndices { name: p, concatenate: false } } /
            non_posix_extensions_enabled() p:variable_name() "[*]" { Parameter::NamedWithAllIndices { name: p, concatenate: true } } /
            non_posix_extensions_enabled() p:variable_name() "[" index:$(arithmetic_word(<"]">)) "]" {?
                Ok(Parameter::NamedWithIndex { name: p, index })
            } /
            p:variable_name() { Parameter::Named(p) }

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

        rule variable_name() -> WordString =
            $(!['0'..='9'] ['_' | '0'..='9' | 'a'..='z' | 'A'..='Z']+)

        pub(crate) rule command_substitution() -> WordPiece =
            "$(" c:command() ")" { WordPiece::CommandSubstitution(c) } /
            "`" c:backquoted_command() "`" { WordPiece::BackquotedCommandSubstitution(c) }

        pub(crate) rule command() -> WordString = $(command_piece()*)

        pub(crate) rule command_piece() -> () =
            word_piece(<[')']>, true /*in_command*/) {} /
            ([' ' | '\t'])+ {}

        rule backquoted_command() -> WordString =
            chars:(backquoted_char()*) { chars.into_iter().collect() }

        rule backquoted_char() -> char =
            "\\`" { '`' } /
            [^'`']

        rule arithmetic_expansion() -> WordPiece =
            "$((" e:$(arithmetic_word(<"))">)) "))" { WordPiece::ArithmeticExpression(ast::UnexpandedArithmeticExpr { value: e } ) }

        rule substring_offset() -> ast::UnexpandedArithmeticExpr =
            s:$(arithmetic_word(<[':' | '}']>)) { ast::UnexpandedArithmeticExpr { value: s } }

        rule substring_length() -> ast::UnexpandedArithmeticExpr =
            s:$(arithmetic_word(<[':' | '}']>)) { ast::UnexpandedArithmeticExpr { value: s } }

        rule parameter_replacement_str() -> WordString =
            "/" s:$(word(<['}']>)) { s }

        rule parameter_search_pattern() -> WordString =
            s:$(word(<['}' | '/']>)) { s }

        rule parameter_expression_word() -> WordString =
            s:$(word(<['}']>)) { s }

        rule extglob_enabled() -> () =
            &[_] {? if parser_options.enable_extended_globbing { Ok(()) } else { Err("no extglob") } }

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
        super::expansion_parser::command_piece(&"echo".into(), &ParserOptions::default())?;
        super::expansion_parser::command_piece(&"hi".into(), &ParserOptions::default())?;
        super::expansion_parser::command(&"echo hi".into(), &ParserOptions::default())?;
        super::expansion_parser::command_substitution(
            &"$(echo hi)".into(),
            &ParserOptions::default(),
        )?;

        let parsed = super::parse(&"$(echo hi)".into(), &ParserOptions::default())?;
        assert_matches!(
            &parsed[..],
            [WordPieceWithSource { piece: WordPiece::CommandSubstitution(s), .. }] if s.as_str() == "echo hi"
        );

        Ok(())
    }

    #[test]
    fn parse_command_substitution_with_embedded_quotes() -> Result<()> {
        super::expansion_parser::command_piece(&"echo".into(), &ParserOptions::default())?;
        super::expansion_parser::command_piece(&r#""hi""#.into(), &ParserOptions::default())?;
        super::expansion_parser::command(&r#"echo "hi""#.into(), &ParserOptions::default())?;
        super::expansion_parser::command_substitution(
            &r#"$(echo "hi")"#.into(),
            &ParserOptions::default(),
        )?;

        let parsed = super::parse(&r#"$(echo "hi")"#.into(), &ParserOptions::default())?;
        assert_matches!(
            &parsed[..],
            [WordPieceWithSource { piece: WordPiece::CommandSubstitution(s), .. }] if s.as_str() == r#"echo "hi""#
        );

        Ok(())
    }

    #[test]
    fn parse_command_substitution_with_embedded_extglob() -> Result<()> {
        let parsed = super::parse(&"$(echo !(x))".into(), &ParserOptions::default())?;
        assert_matches!(
            &parsed[..],
            [WordPieceWithSource { piece: WordPiece::CommandSubstitution(s), .. }] if s.as_str() == "echo !(x)"
        );

        Ok(())
    }

    #[test]
    fn parse_extglob_with_embedded_parameter() -> Result<()> {
        let parsed = super::parse(&"+([$var])".into(), &ParserOptions::default())?;
        assert_matches!(
            &parsed[..],
            [WordPieceWithSource { piece: WordPiece::Text(s1), .. },
             WordPieceWithSource { piece: WordPiece::ParameterExpansion(ParameterExpr::Parameter { parameter: Parameter::Named(s2), .. }), ..},
             WordPieceWithSource { piece: WordPiece::Text(s3), .. }] if s1 == "+([" && s2 == "var" && s3 == "])"
        );

        Ok(())
    }
}
