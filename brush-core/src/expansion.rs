use std::cmp::min;

use brush_parser::ast;
use brush_parser::word::ParameterTransformOp;
use brush_parser::word::SubstringMatchKind;
use itertools::Itertools;

use crate::arithmetic::ExpandAndEvaluate;
use crate::env;
use crate::error;
use crate::escape;
use crate::interp::ProcessGroupPolicy;
use crate::openfiles;
use crate::patterns;
use crate::prompt;
use crate::shell::Shell;
use crate::sys;
use crate::trace_categories;
use crate::variables::ShellValueUnsetType;
use crate::variables::ShellVariable;
use crate::variables::{self, ShellValue};

#[derive(Debug)]
struct Expansion {
    fields: Vec<WordField>,
    concatenate: bool,
    undefined: bool,
}

impl Default for Expansion {
    fn default() -> Self {
        Self {
            fields: vec![],
            concatenate: true,
            undefined: false,
        }
    }
}

impl From<Expansion> for String {
    fn from(value: Expansion) -> Self {
        // TODO: Use IFS instead for separator?
        value.fields.into_iter().map(String::from).join(" ")
    }
}

impl From<String> for Expansion {
    fn from(value: String) -> Self {
        Self {
            fields: vec![WordField::from(value)],
            ..Expansion::default()
        }
    }
}

impl From<ExpansionPiece> for Expansion {
    fn from(piece: ExpansionPiece) -> Self {
        Self {
            fields: vec![WordField::from(piece)],
            ..Expansion::default()
        }
    }
}

impl Expansion {
    pub(crate) fn classify(&self) -> ParameterState {
        let non_empty = self
            .fields
            .iter()
            .any(|field| field.0.iter().any(|piece| !piece.as_str().is_empty()));

        if self.undefined {
            ParameterState::Undefined
        } else if non_empty {
            ParameterState::NonZeroLength
        } else {
            ParameterState::DefinedEmptyString
        }
    }

    pub(crate) fn undefined() -> Self {
        Self {
            fields: vec![WordField::from(String::new())],
            concatenate: true,
            undefined: true,
        }
    }

    pub(crate) fn polymorphic_len(&self) -> usize {
        if self.fields.len() > 1 {
            self.fields.len()
        } else {
            self.fields.iter().fold(0, |acc, field| acc + field.len())
        }
    }

    pub(crate) fn polymorphic_subslice(&self, index: usize, end: usize) -> Self {
        let len = end - index;

        if self.fields.len() > 1 {
            let actual_len = min(len, self.fields.len() - index);
            let fields = self.fields[index..(index + actual_len)].to_vec();

            Expansion {
                fields,
                concatenate: self.concatenate,
                undefined: self.undefined,
            }
        } else {
            let mut fields = vec![];

            let mut offset = index;
            let mut left = len;
            for field in &self.fields {
                let mut pieces = vec![];

                for piece in &field.0 {
                    if left == 0 {
                        break;
                    }

                    let piece_str = piece.as_str();
                    if offset < piece_str.len() {
                        let len_from_this_piece = min(left, piece_str.len() - offset);

                        let new_piece = match piece {
                            ExpansionPiece::Unsplittable(s) => ExpansionPiece::Unsplittable(
                                s[offset..(offset + len_from_this_piece)].to_owned(),
                            ),
                            ExpansionPiece::Splittable(s) => ExpansionPiece::Splittable(
                                s[offset..(offset + len_from_this_piece)].to_owned(),
                            ),
                        };

                        pieces.push(new_piece);

                        left -= len_from_this_piece;
                    }

                    offset += piece_str.len();
                }

                if !pieces.is_empty() {
                    fields.push(WordField(pieces));
                }
            }

            Expansion {
                fields,
                concatenate: self.concatenate,
                undefined: self.undefined,
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct WordField(Vec<ExpansionPiece>);

impl WordField {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn len(&self) -> usize {
        self.0.iter().fold(0, |acc, piece| acc + piece.len())
    }
}

impl From<WordField> for String {
    fn from(field: WordField) -> Self {
        field.0.into_iter().map(String::from).collect()
    }
}

impl From<WordField> for patterns::Pattern {
    fn from(value: WordField) -> Self {
        let pieces: Vec<_> = value
            .0
            .into_iter()
            .map(patterns::PatternPiece::from)
            .collect();

        patterns::Pattern::from(pieces)
    }
}

impl From<ExpansionPiece> for WordField {
    fn from(piece: ExpansionPiece) -> Self {
        Self(vec![piece])
    }
}

impl From<String> for WordField {
    fn from(value: String) -> Self {
        Self(vec![ExpansionPiece::Splittable(value)])
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ExpansionPiece {
    Unsplittable(String),
    Splittable(String),
}

impl From<ExpansionPiece> for String {
    fn from(piece: ExpansionPiece) -> Self {
        match piece {
            ExpansionPiece::Unsplittable(s) => s,
            ExpansionPiece::Splittable(s) => s,
        }
    }
}

impl From<ExpansionPiece> for patterns::PatternPiece {
    fn from(piece: ExpansionPiece) -> Self {
        match piece {
            ExpansionPiece::Unsplittable(s) => patterns::PatternPiece::Literal(s),
            ExpansionPiece::Splittable(s) => patterns::PatternPiece::Pattern(s),
        }
    }
}

impl From<ExpansionPiece> for crate::regex::RegexPiece {
    fn from(piece: ExpansionPiece) -> Self {
        match piece {
            ExpansionPiece::Unsplittable(s) => crate::regex::RegexPiece::Literal(s),
            ExpansionPiece::Splittable(s) => crate::regex::RegexPiece::Pattern(s),
        }
    }
}

impl ExpansionPiece {
    fn as_str(&self) -> &str {
        match self {
            ExpansionPiece::Unsplittable(s) => s.as_str(),
            ExpansionPiece::Splittable(s) => s.as_str(),
        }
    }

    fn len(&self) -> usize {
        match self {
            ExpansionPiece::Unsplittable(s) => s.len(),
            ExpansionPiece::Splittable(s) => s.len(),
        }
    }

    fn make_unsplittable(self) -> ExpansionPiece {
        match self {
            ExpansionPiece::Unsplittable(_) => self,
            ExpansionPiece::Splittable(s) => ExpansionPiece::Unsplittable(s),
        }
    }
}

enum ParameterState {
    Undefined,
    DefinedEmptyString,
    NonZeroLength,
}

pub(crate) async fn basic_expand_pattern(
    shell: &mut Shell,
    word: &ast::Word,
) -> Result<patterns::Pattern, error::Error> {
    let mut expander = WordExpander::new(shell);
    expander.basic_expand_pattern(&word.flatten()).await
}

pub(crate) async fn basic_expand_regex(
    shell: &mut Shell,
    word: &ast::Word,
) -> Result<crate::regex::Regex, error::Error> {
    let mut expander = WordExpander::new(shell);
    expander.basic_expand_regex(&word.flatten()).await
}

pub(crate) async fn basic_expand_word(
    shell: &mut Shell,
    word: &ast::Word,
) -> Result<String, error::Error> {
    basic_expand_str(shell, word.flatten().as_str()).await
}

pub(crate) async fn basic_expand_str(shell: &mut Shell, s: &str) -> Result<String, error::Error> {
    let mut expander = WordExpander::new(shell);
    expander.basic_expand_to_str(s).await
}

pub(crate) async fn basic_expand_str_without_tilde(
    shell: &mut Shell,
    s: &str,
) -> Result<String, error::Error> {
    let mut expander = WordExpander::new(shell);
    expander.parser_options.tilde_expansion = false;
    expander.basic_expand_to_str(s).await
}

pub(crate) async fn full_expand_and_split_word(
    shell: &mut Shell,
    word: &ast::Word,
) -> Result<Vec<String>, error::Error> {
    full_expand_and_split_str(shell, word.flatten().as_str()).await
}

pub(crate) async fn full_expand_and_split_str(
    shell: &mut Shell,
    s: &str,
) -> Result<Vec<String>, error::Error> {
    let mut expander = WordExpander::new(shell);
    expander.full_expand_with_splitting(s).await
}

pub(crate) async fn assign_to_named_parameter(
    shell: &mut Shell,
    name: &str,
    value: String,
) -> Result<(), error::Error> {
    let parser_options = shell.parser_options();
    let mut expander = WordExpander::new(shell);
    let parameter = brush_parser::word::parse_parameter(name, &parser_options)?;
    expander.assign_to_parameter(&parameter, value).await
}

struct WordExpander<'a> {
    shell: &'a mut Shell,
    parser_options: brush_parser::ParserOptions,
}

impl<'a> WordExpander<'a> {
    pub fn new(shell: &'a mut Shell) -> Self {
        let parser_options = shell.parser_options();

        Self {
            shell,
            parser_options,
        }
    }

    /// Apply tilde-expansion, parameter expansion, command substitution, and arithmetic expansion.
    pub async fn basic_expand_to_str(&mut self, word: &str) -> Result<String, error::Error> {
        let expanded = String::from(self.basic_expand(word).await?);
        Ok(expanded)
    }

    async fn basic_expand_opt_pattern(
        &mut self,
        word: &Option<String>,
    ) -> Result<Option<patterns::Pattern>, error::Error> {
        if let Some(word) = word {
            Ok(Some(self.basic_expand_pattern(word).await?))
        } else {
            Ok(None)
        }
    }

    async fn basic_expand_pattern(
        &mut self,
        word: &str,
    ) -> Result<patterns::Pattern, error::Error> {
        let expansion = self.basic_expand(word).await?;

        // TODO: Use IFS instead for separator?
        #[allow(unstable_name_collisions)]
        let pattern_pieces: Vec<_> = expansion
            .fields
            .into_iter()
            .map(|field| {
                field
                    .0
                    .into_iter()
                    .map(patterns::PatternPiece::from)
                    .collect::<Vec<_>>()
            })
            .intersperse(vec![patterns::PatternPiece::Literal(String::from(" "))])
            .flatten()
            .collect();

        Ok(patterns::Pattern::from(pattern_pieces))
    }

    async fn basic_expand_regex(
        &mut self,
        word: &str,
    ) -> Result<crate::regex::Regex, error::Error> {
        let expansion = self.basic_expand(word).await?;

        // TODO: Use IFS instead for separator?
        #[allow(unstable_name_collisions)]
        let regex_pieces: Vec<_> = expansion
            .fields
            .into_iter()
            .map(|field| {
                field
                    .0
                    .into_iter()
                    .map(crate::regex::RegexPiece::from)
                    .collect::<Vec<_>>()
            })
            .intersperse(vec![crate::regex::RegexPiece::Literal(String::from(" "))])
            .flatten()
            .collect();

        Ok(crate::regex::Regex::from(regex_pieces))
    }

    /// Apply tilde-expansion, parameter expansion, command substitution, and arithmetic expansion;
    /// yield pieces that could be further processed.
    async fn basic_expand(&mut self, word: &str) -> Result<Expansion, error::Error> {
        tracing::debug!(target: trace_categories::EXPANSION, "Basic expanding: '{word}'");

        //
        // TODO: Brace expansion in unquoted pieces
        // Issue #42
        //

        //
        // Expand: tildes, parameters, command substitutions, arithmetic.
        //
        let pieces = brush_parser::word::parse(word, &self.parser_options)?;

        let mut expansions = vec![];
        for piece in pieces {
            let piece_expansion = self.expand_word_piece(piece.piece).await?;
            expansions.push(piece_expansion);
        }

        Ok(coalesce_expansions(expansions))
    }

    /// Apply tilde-expansion, parameter expansion, command substitution, and arithmetic expansion;
    /// then perform field splitting and pathname expansion.
    pub async fn full_expand_with_splitting(
        &mut self,
        word: &str,
    ) -> Result<Vec<String>, error::Error> {
        // Perform basic expansion first.
        let basic_expansion = self.basic_expand(word).await?;

        // Then split.
        let fields: Vec<WordField> = self.split_fields(basic_expansion);

        // Now expand pathnames if necessary. This also unquotes as a side effect.
        let result = fields
            .into_iter()
            .flat_map(|field| {
                if self.shell.options.disable_filename_globbing {
                    vec![String::from(field)]
                } else {
                    self.expand_pathnames_in_field(field)
                }
            })
            .collect();

        Ok(result)
    }

    fn split_fields(&self, expansion: Expansion) -> Vec<WordField> {
        let ifs = self.shell.get_ifs();

        let mut fields: Vec<WordField> = vec![];
        let mut current_field = WordField::new();

        // Go through the fields we have so far.
        for existing_field in expansion.fields {
            for piece in existing_field.0 {
                match piece {
                    ExpansionPiece::Unsplittable(_) => current_field.0.push(piece),
                    ExpansionPiece::Splittable(s) => {
                        for c in s.chars() {
                            if ifs.contains(c) {
                                if !current_field.0.is_empty() {
                                    fields.push(std::mem::take(&mut current_field));
                                }
                            } else {
                                match current_field.0.last_mut() {
                                    Some(ExpansionPiece::Splittable(last)) => last.push(c),
                                    Some(ExpansionPiece::Unsplittable(_)) | None => {
                                        current_field
                                            .0
                                            .push(ExpansionPiece::Splittable(c.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !current_field.0.is_empty() {
                fields.push(std::mem::take(&mut current_field));
            }
        }

        fields
    }

    fn expand_pathnames_in_field(&self, field: WordField) -> Vec<String> {
        let pattern = patterns::Pattern::from(field.clone());
        let expansions = pattern
            .expand(
                self.shell.working_dir.as_path(),
                self.parser_options.enable_extended_globbing,
                Some(&patterns::Pattern::accept_all_expand_filter),
            )
            .unwrap_or_default();

        if expansions.is_empty() {
            vec![String::from(field)]
        } else {
            expansions
        }
    }

    #[async_recursion::async_recursion]
    #[allow(clippy::too_many_lines)]
    async fn expand_word_piece(
        &mut self,
        word_piece: brush_parser::word::WordPiece,
    ) -> Result<Expansion, error::Error> {
        let expansion: Expansion = match word_piece {
            brush_parser::word::WordPiece::Text(s) => {
                Expansion::from(ExpansionPiece::Splittable(s))
            }
            brush_parser::word::WordPiece::SingleQuotedText(s) => {
                Expansion::from(ExpansionPiece::Unsplittable(s))
            }
            brush_parser::word::WordPiece::AnsiCQuotedText(s) => {
                let (expanded, _) = escape::expand_backslash_escapes(
                    s.as_str(),
                    escape::EscapeExpansionMode::AnsiCQuotes,
                )?;
                Expansion::from(ExpansionPiece::Unsplittable(
                    String::from_utf8_lossy(expanded.as_slice()).into_owned(),
                ))
            }
            brush_parser::word::WordPiece::DoubleQuotedSequence(pieces) => {
                let mut fields: Vec<WordField> = vec![];

                let pieces_is_empty = pieces.is_empty();

                for piece in pieces {
                    let Expansion {
                        fields: this_fields,
                        concatenate,
                        undefined: _undefined,
                    } = self.expand_word_piece(piece.piece).await?;

                    let fields_to_append = if concatenate {
                        #[allow(unstable_name_collisions)]
                        let mut concatenated: Vec<ExpansionPiece> = this_fields
                            .into_iter()
                            .map(|WordField(pieces)| {
                                pieces
                                    .into_iter()
                                    .map(|piece| piece.make_unsplittable())
                                    .collect()
                            })
                            .intersperse(vec![ExpansionPiece::Unsplittable(String::from(" "))])
                            .flatten()
                            .collect();

                        // If there were no pieces, make sure there's an empty string after
                        // concatenation.
                        if concatenated.is_empty() {
                            concatenated.push(ExpansionPiece::Splittable(String::new()));
                        }

                        vec![WordField(concatenated)]
                    } else {
                        this_fields
                    };

                    for (i, WordField(next_pieces)) in fields_to_append.into_iter().enumerate() {
                        // Flip to unsplittable.
                        let mut next_pieces: Vec<_> = next_pieces
                            .into_iter()
                            .map(|piece| piece.make_unsplittable())
                            .collect();

                        if i == 0 {
                            if let Some(WordField(last_pieces)) = fields.last_mut() {
                                last_pieces.append(&mut next_pieces);
                                continue;
                            }
                        }

                        fields.push(WordField(next_pieces));
                    }
                }

                // If there were no pieces, then make sure we yield a single field containing an
                // empty, unsplittable string.
                if pieces_is_empty {
                    fields.push(WordField::from(ExpansionPiece::Unsplittable(String::new())));
                }

                Expansion {
                    fields,
                    concatenate: false,
                    undefined: false,
                }
            }
            brush_parser::word::WordPiece::TildePrefix(prefix) => Expansion::from(
                ExpansionPiece::Unsplittable(self.expand_tilde_expression(prefix.as_str())?),
            ),
            brush_parser::word::WordPiece::ParameterExpansion(p) => {
                self.expand_parameter_expr(p).await?
            }
            brush_parser::word::WordPiece::BackquotedCommandSubstitution(s)
            | brush_parser::word::WordPiece::CommandSubstitution(s) => {
                // Insantiate a subshell to run the command in.
                let mut subshell = self.shell.clone();

                // Set up pipe so we can read the output.
                let (reader, writer) = sys::pipes::pipe()?;
                subshell
                    .open_files
                    .files
                    .insert(1, openfiles::OpenFile::PipeWriter(writer));

                let mut params = subshell.default_exec_params();
                params.process_group_policy = ProcessGroupPolicy::SameProcessGroup;

                // Run the command.
                let result = subshell.run_string(s, &params).await?;

                // Make sure the subshell and params are closed; among other things, this
                // ensures they're not holding onto the write end of the pipe.
                drop(subshell);
                drop(params);

                // Store the status.
                self.shell.last_exit_status = result.exit_code;

                // Extract output.
                let output_str = std::io::read_to_string(reader)?;

                // We trim trailing newlines, per spec.
                let output_str = output_str.trim_end_matches('\n');

                Expansion::from(ExpansionPiece::Splittable(output_str.to_owned()))
            }
            brush_parser::word::WordPiece::EscapeSequence(s) => {
                let expanded = s.strip_prefix('\\').unwrap();
                Expansion::from(ExpansionPiece::Unsplittable(expanded.to_owned()))
            }
            brush_parser::word::WordPiece::ArithmeticExpression(e) => Expansion::from(
                ExpansionPiece::Splittable(self.expand_arithmetic_expr(e).await?),
            ),
        };

        Ok(expansion)
    }

    fn expand_tilde_expression(&self, prefix: &str) -> Result<String, error::Error> {
        if !prefix.is_empty() {
            Ok(sys::users::get_user_home_dir(prefix).map_or_else(
                || std::format!("~{prefix}"),
                |p| p.to_string_lossy().to_string(),
            ))
        } else if let Some(home_dir) = self.shell.get_home_dir() {
            Ok(home_dir.to_string_lossy().to_string())
        } else {
            Err(error::Error::TildeWithoutValidHome)
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn expand_parameter_expr(
        &mut self,
        expr: brush_parser::word::ParameterExpr,
    ) -> Result<Expansion, error::Error> {
        #[allow(clippy::cast_possible_truncation)]
        match expr {
            brush_parser::word::ParameterExpr::Parameter {
                parameter,
                indirect,
            } => self.expand_parameter(&parameter, indirect).await,
            brush_parser::word::ParameterExpr::UseDefaultValues {
                parameter,
                indirect,
                test_type,
                default_value,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let default_value = default_value.as_ref().map_or_else(|| "", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(expanded_parameter),
                    _ => Ok(Expansion::from(
                        self.basic_expand_to_str(default_value).await?,
                    )),
                }
            }
            brush_parser::word::ParameterExpr::AssignDefaultValues {
                parameter,
                indirect,
                test_type,
                default_value,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let default_value = default_value.as_ref().map_or_else(|| "", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(expanded_parameter),
                    _ => {
                        let expanded_default_value =
                            self.basic_expand_to_str(default_value).await?;
                        self.assign_to_parameter(&parameter, expanded_default_value.clone())
                            .await?;
                        Ok(Expansion::from(expanded_default_value))
                    }
                }
            }
            brush_parser::word::ParameterExpr::IndicateErrorIfNullOrUnset {
                parameter,
                indirect,
                test_type,
                error_message,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let error_message = error_message.as_ref().map_or_else(|| "", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(expanded_parameter),
                    _ => Err(error::Error::CheckedExpansionError(
                        self.basic_expand_to_str(error_message).await?,
                    )),
                }
            }
            brush_parser::word::ParameterExpr::UseAlternativeValue {
                parameter,
                indirect,
                test_type,
                alternative_value,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let alternative_value = alternative_value
                    .as_ref()
                    .map_or_else(|| "", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(self.basic_expand(alternative_value).await?),
                    _ => Ok(Expansion::from(String::new())),
                }
            }
            brush_parser::word::ParameterExpr::ParameterLength {
                parameter,
                indirect,
            } => {
                let expansion = self.expand_parameter(&parameter, indirect).await?;
                Ok(Expansion::from(expansion.polymorphic_len().to_string()))
            }
            brush_parser::word::ParameterExpr::RemoveSmallestSuffixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter: String =
                    self.expand_parameter(&parameter, indirect).await?.into();
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;
                let result = patterns::remove_smallest_matching_suffix(
                    expanded_parameter.as_str(),
                    &expanded_pattern,
                    self.parser_options.enable_extended_globbing,
                )?;
                Ok(Expansion::from(result.to_owned()))
            }
            brush_parser::word::ParameterExpr::RemoveLargestSuffixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter: String =
                    self.expand_parameter(&parameter, indirect).await?.into();
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;
                let result = patterns::remove_largest_matching_suffix(
                    expanded_parameter.as_str(),
                    &expanded_pattern,
                    self.parser_options.enable_extended_globbing,
                )?;

                Ok(Expansion::from(result.to_owned()))
            }
            brush_parser::word::ParameterExpr::RemoveSmallestPrefixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter: String =
                    self.expand_parameter(&parameter, indirect).await?.into();
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;
                let result = patterns::remove_smallest_matching_prefix(
                    expanded_parameter.as_str(),
                    &expanded_pattern,
                    self.parser_options.enable_extended_globbing,
                )?;

                Ok(Expansion::from(result.to_owned()))
            }
            brush_parser::word::ParameterExpr::RemoveLargestPrefixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter: String =
                    self.expand_parameter(&parameter, indirect).await?.into();
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;
                let result = patterns::remove_largest_matching_prefix(
                    expanded_parameter.as_str(),
                    &expanded_pattern,
                    self.parser_options.enable_extended_globbing,
                )?;

                Ok(Expansion::from(result.to_owned()))
            }
            brush_parser::word::ParameterExpr::Substring {
                parameter,
                indirect,
                offset,
                length,
            } => {
                let mut expanded_parameter = self.expand_parameter(&parameter, indirect).await?;

                // If this is ${@:...} then make sure $0 is in the array being sliced.
                if matches!(
                    parameter,
                    brush_parser::word::Parameter::Special(
                        brush_parser::word::SpecialParameter::AllPositionalParameters {
                            concatenate: _
                        },
                    )
                ) {
                    let shell_name = self
                        .shell
                        .shell_name
                        .as_ref()
                        .map_or_else(|| "", |name| name.as_str());

                    expanded_parameter.fields.insert(
                        0,
                        WordField::from(ExpansionPiece::Splittable(shell_name.to_owned())),
                    );
                }

                let expanded_offset = offset.eval(self.shell, false).await?;
                let expanded_offset = usize::try_from(expanded_offset)?;

                let expanded_parameter_len = expanded_parameter.polymorphic_len();
                if expanded_offset >= expanded_parameter_len {
                    return Ok(Expansion::from(String::new()));
                }

                let end_offset = if let Some(length) = length {
                    let mut expanded_length = length.eval(self.shell, false).await?;
                    if expanded_length < 0 {
                        let param_length: i64 = i64::try_from(expanded_parameter_len)?;
                        expanded_length += param_length;
                    }

                    let expanded_length = std::cmp::min(
                        usize::try_from(expanded_length)?,
                        expanded_parameter_len - expanded_offset,
                    );

                    expanded_offset + expanded_length
                } else {
                    expanded_parameter_len
                };

                Ok(expanded_parameter.polymorphic_subslice(expanded_offset, end_offset))
            }
            brush_parser::word::ParameterExpr::Transform {
                parameter,
                indirect,
                op: ParameterTransformOp::ToAttributeFlags,
            } => {
                if let (_, _, Some(var)) = self
                    .try_resolve_parameter_to_variable(&parameter, indirect)
                    .await?
                {
                    Ok(var.get_attribute_flags().into())
                } else {
                    Ok(String::new().into())
                }
            }
            brush_parser::word::ParameterExpr::Transform {
                parameter,
                indirect,
                op: ParameterTransformOp::ToAssignmentLogic,
            } => {
                if let (Some(name), index, Some(var)) = self
                    .try_resolve_parameter_to_variable(&parameter, indirect)
                    .await?
                {
                    let assignable_value_str = var.value().to_assignable_str(index.as_deref());

                    let mut attr_str = var.get_attribute_flags();
                    if attr_str.is_empty() {
                        attr_str.push('-');
                    }

                    match var.value() {
                        ShellValue::IndexedArray(_)
                        | ShellValue::AssociativeArray(_)
                        | ShellValue::Random => {
                            let equals_or_nothing = if assignable_value_str.is_empty() {
                                ""
                            } else {
                                "="
                            };

                            Ok(std::format!(
                            "declare -{attr_str} {name}{equals_or_nothing}{assignable_value_str}"
                        )
                            .into())
                        }
                        ShellValue::String(_) => {
                            Ok(std::format!("{name}={assignable_value_str}",).into())
                        }
                        ShellValue::Unset(_) => {
                            Ok(std::format!("declare -{attr_str} {name}").into())
                        }
                    }
                } else {
                    Ok(String::new().into())
                }
            }
            brush_parser::word::ParameterExpr::Transform {
                parameter,
                indirect,
                op,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;

                transform_expansion(expanded_parameter, |s| {
                    self.apply_transform_to(&op, s.as_str())
                })
            }
            brush_parser::word::ParameterExpr::UppercaseFirstChar {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;

                transform_expansion(expanded_parameter, |s| {
                    self.uppercase_first_char(s, &expanded_pattern)
                })
            }
            brush_parser::word::ParameterExpr::UppercasePattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;

                transform_expansion(expanded_parameter, |s| {
                    self.uppercase_pattern(s.as_str(), &expanded_pattern)
                })
            }
            brush_parser::word::ParameterExpr::LowercaseFirstChar {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;

                Ok(transform_expansion(expanded_parameter, |s| {
                    self.lowercase_first_char(s, &expanded_pattern)
                })?)
            }
            brush_parser::word::ParameterExpr::LowercasePattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let expanded_pattern = self.basic_expand_opt_pattern(&pattern).await?;

                Ok(transform_expansion(expanded_parameter, |s| {
                    self.lowercase_pattern(s.as_str(), &expanded_pattern)
                })?)
            }
            brush_parser::word::ParameterExpr::ReplaceSubstring {
                parameter,
                indirect,
                pattern,
                replacement,
                match_kind,
            } => {
                let expanded_parameter = self.expand_parameter(&parameter, indirect).await?;
                let expanded_pattern = self.basic_expand_to_str(&pattern).await?;

                // If no replacement was provided, then we replace with an empty string.
                let replacement = replacement.unwrap_or(String::new());
                let expanded_replacement = self.basic_expand_to_str(&replacement).await?;

                let regex = patterns::pattern_to_regex(
                    expanded_pattern.as_str(),
                    matches!(match_kind, brush_parser::word::SubstringMatchKind::Prefix),
                    matches!(match_kind, brush_parser::word::SubstringMatchKind::Suffix),
                    self.parser_options.enable_extended_globbing,
                )?;

                transform_expansion(expanded_parameter, |s| {
                    Self::replace_substring(
                        s.as_str(),
                        &regex,
                        expanded_replacement.as_str(),
                        &match_kind,
                    )
                })
            }
            brush_parser::word::ParameterExpr::VariableNames {
                prefix,
                concatenate,
            } => {
                if prefix.is_empty() {
                    Ok(Expansion::from(String::new()))
                } else {
                    let matching_names = self
                        .shell
                        .env
                        .iter()
                        .filter_map(|(name, _)| {
                            if name.starts_with(prefix.as_str()) {
                                Some(name.to_owned())
                            } else {
                                None
                            }
                        })
                        .sorted();

                    Ok(Expansion {
                        fields: matching_names
                            .into_iter()
                            .map(|name| WordField(vec![ExpansionPiece::Splittable(name)]))
                            .collect(),
                        concatenate,
                        undefined: false,
                    })
                }
            }
            brush_parser::word::ParameterExpr::MemberKeys {
                variable_name,
                concatenate,
            } => {
                let keys = if let Some((_, var)) = self.shell.env.get(variable_name) {
                    var.value().get_element_keys()
                } else {
                    vec![]
                };

                Ok(Expansion {
                    fields: keys
                        .into_iter()
                        .map(|key| WordField(vec![ExpansionPiece::Splittable(key)]))
                        .collect(),
                    concatenate,
                    undefined: false,
                })
            }
        }
    }

    async fn assign_to_parameter(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        value: String,
    ) -> Result<(), error::Error> {
        let (variable_name, index) = match parameter {
            brush_parser::word::Parameter::Named(name) => (name, None),
            brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                let is_set_assoc_array = if let Some((_, var)) = self.shell.env.get(name.as_str()) {
                    matches!(
                        var.value(),
                        ShellValue::AssociativeArray(_)
                            | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
                    )
                } else {
                    false
                };

                let index_to_use = self
                    .expand_array_index(index.as_str(), is_set_assoc_array)
                    .await?;
                (name, Some(index_to_use))
            }
            brush_parser::word::Parameter::Positional(_)
            | brush_parser::word::Parameter::NamedWithAllIndices {
                name: _,
                concatenate: _,
            }
            | brush_parser::word::Parameter::Special(_) => {
                return Err(error::Error::CannotAssignToSpecialParameter);
            }
        };

        if let Some(index) = index {
            self.shell.env.update_or_add_array_element(
                variable_name,
                index,
                value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )
        } else {
            self.shell.env.update_or_add(
                variable_name,
                variables::ShellValueLiteral::Scalar(value),
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )
        }
    }

    async fn try_resolve_parameter_to_variable(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        indirect: bool,
    ) -> Result<(Option<String>, Option<String>, Option<ShellVariable>), error::Error> {
        if !indirect {
            Ok(self.try_resolve_parameter_to_variable_without_indirect(parameter))
        } else {
            let expansion = self.expand_parameter(parameter, true).await?;
            let parameter_str: String = expansion.into();
            let inner_parameter =
                brush_parser::word::parse_parameter(parameter_str.as_str(), &self.parser_options)?;
            Ok(self.try_resolve_parameter_to_variable_without_indirect(&inner_parameter))
        }
    }

    fn try_resolve_parameter_to_variable_without_indirect(
        &self,
        parameter: &brush_parser::word::Parameter,
    ) -> (Option<String>, Option<String>, Option<ShellVariable>) {
        let (name, index) = match parameter {
            brush_parser::word::Parameter::Positional(_)
            | brush_parser::word::Parameter::Special(_) => (None, None),
            brush_parser::word::Parameter::Named(name) => (Some(name.to_owned()), Some("0".into())),
            brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                (Some(name.to_owned()), Some(index.to_owned()))
            }
            brush_parser::word::Parameter::NamedWithAllIndices {
                name,
                concatenate: _concatenate,
            } => (Some(name.to_owned()), None),
        };

        let var = name
            .as_ref()
            .and_then(|name| self.shell.env.get(name).map(|(_, var)| var.clone()));

        (name, index, var)
    }

    async fn expand_parameter(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        indirect: bool,
    ) -> Result<Expansion, error::Error> {
        let expansion = self.expand_parameter_without_indirect(parameter).await?;
        if !indirect {
            Ok(expansion)
        } else {
            let parameter_str: String = expansion.into();
            let inner_parameter =
                brush_parser::word::parse_parameter(parameter_str.as_str(), &self.parser_options)?;

            self.expand_parameter_without_indirect(&inner_parameter)
                .await
        }
    }

    async fn expand_parameter_without_indirect(
        &mut self,
        parameter: &brush_parser::word::Parameter,
    ) -> Result<Expansion, error::Error> {
        match parameter {
            brush_parser::word::Parameter::Positional(p) => {
                if *p == 0 {
                    self.expand_special_parameter(&brush_parser::word::SpecialParameter::ShellName)
                } else if let Some(parameter) =
                    self.shell.positional_parameters.get((p - 1) as usize)
                {
                    Ok(Expansion::from(parameter.to_owned()))
                } else {
                    Ok(Expansion::undefined())
                }
            }
            brush_parser::word::Parameter::Special(s) => self.expand_special_parameter(s),
            brush_parser::word::Parameter::Named(n) => {
                if !valid_variable_name(n.as_str()) {
                    Err(error::Error::BadSubstitution)
                } else if let Some((_, var)) = self.shell.env.get(n) {
                    if matches!(var.value(), ShellValue::Unset(_)) {
                        Ok(Expansion::undefined())
                    } else {
                        Ok(Expansion::from(var.value().to_cow_string().to_string()))
                    }
                } else {
                    Ok(Expansion::undefined())
                }
            }
            brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                // First check to see if it's an associative array.
                let is_set_assoc_array = if let Some((_, var)) = self.shell.env.get(name.as_str()) {
                    matches!(
                        var.value(),
                        ShellValue::AssociativeArray(_)
                            | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
                    )
                } else {
                    false
                };

                // Figure out which index to use.
                let index_to_use = self
                    .expand_array_index(index.as_str(), is_set_assoc_array)
                    .await?;

                // Index into the array.
                if let Some((_, var)) = self.shell.env.get(name.as_str()) {
                    if let Some(value) = var.value().get_at(index_to_use.as_str())? {
                        Ok(Expansion::from(value.to_string()))
                    } else {
                        Ok(Expansion::undefined())
                    }
                } else {
                    Ok(Expansion::undefined())
                }
            }
            brush_parser::word::Parameter::NamedWithAllIndices { name, concatenate } => {
                if let Some((_, var)) = self.shell.env.get(name) {
                    let values = var.value().get_element_values();

                    Ok(Expansion {
                        fields: values
                            .into_iter()
                            .map(|value| WordField(vec![ExpansionPiece::Splittable(value)]))
                            .collect(),
                        concatenate: *concatenate,
                        undefined: false,
                    })
                } else {
                    Ok(Expansion {
                        fields: vec![],
                        concatenate: *concatenate,
                        undefined: false,
                    })
                }
            }
        }
    }

    async fn expand_array_index(
        &mut self,
        index: &str,
        for_set_associative_array: bool,
    ) -> Result<String, error::Error> {
        let index_to_use = if for_set_associative_array {
            self.basic_expand_to_str(index).await?
        } else {
            let index_expr = ast::UnexpandedArithmeticExpr {
                value: index.to_owned(),
            };
            self.expand_arithmetic_expr(index_expr).await?
        };

        Ok(index_to_use)
    }

    #[allow(clippy::unnecessary_wraps)]
    fn expand_special_parameter(
        &mut self,
        parameter: &brush_parser::word::SpecialParameter,
    ) -> Result<Expansion, error::Error> {
        match parameter {
            brush_parser::word::SpecialParameter::AllPositionalParameters { concatenate } => {
                let positional_params = self.shell.positional_parameters.iter();

                Ok(Expansion {
                    fields: positional_params
                        .into_iter()
                        .map(|param| WordField(vec![ExpansionPiece::Splittable(param.to_owned())]))
                        .collect(),
                    concatenate: *concatenate,
                    undefined: false,
                })
            }
            brush_parser::word::SpecialParameter::PositionalParameterCount => Ok(Expansion::from(
                self.shell.positional_parameters.len().to_string(),
            )),
            brush_parser::word::SpecialParameter::LastExitStatus => {
                Ok(Expansion::from(self.shell.last_exit_status.to_string()))
            }
            brush_parser::word::SpecialParameter::CurrentOptionFlags => {
                Ok(Expansion::from(self.shell.current_option_flags()))
            }
            brush_parser::word::SpecialParameter::ProcessId => {
                Ok(Expansion::from(std::process::id().to_string()))
            }
            brush_parser::word::SpecialParameter::LastBackgroundProcessId => {
                if let Some(job) = self.shell.jobs.current_job() {
                    if let Some(pid) = job.get_representative_pid() {
                        return Ok(Expansion::from(pid.to_string()));
                    }
                }
                Ok(Expansion::from(String::new()))
            }
            brush_parser::word::SpecialParameter::ShellName => Ok(Expansion::from(
                self.shell
                    .shell_name
                    .as_ref()
                    .map_or_else(String::new, |name| name.clone()),
            )),
        }
    }

    async fn expand_arithmetic_expr(
        &mut self,
        expr: brush_parser::ast::UnexpandedArithmeticExpr,
    ) -> Result<String, error::Error> {
        let value = expr.eval(self.shell, false).await?;
        Ok(value.to_string())
    }

    #[allow(clippy::unwrap_in_result)]
    fn uppercase_first_char(
        &mut self,
        s: String,
        pattern: &Option<patterns::Pattern>,
    ) -> Result<String, error::Error> {
        if let Some(first_char) = s.chars().next() {
            let applicable = if let Some(pattern) = pattern {
                pattern.is_empty()
                    || pattern.exactly_matches(
                        first_char.to_string().as_str(),
                        self.shell.options.extended_globbing,
                    )?
            } else {
                true
            };

            if applicable {
                let mut result = String::new();
                result.push(first_char.to_uppercase().next().unwrap());
                result.push_str(s.get(1..).unwrap());
                Ok(result)
            } else {
                Ok(s)
            }
        } else {
            Ok(s)
        }
    }

    #[allow(clippy::unwrap_in_result)]
    fn lowercase_first_char(
        &mut self,
        s: String,
        pattern: &Option<patterns::Pattern>,
    ) -> Result<String, error::Error> {
        if let Some(first_char) = s.chars().next() {
            let applicable = if let Some(pattern) = pattern {
                pattern.is_empty()
                    || pattern.exactly_matches(
                        first_char.to_string().as_str(),
                        self.shell.options.extended_globbing,
                    )?
            } else {
                true
            };

            if applicable {
                let mut result = String::new();
                result.push(first_char.to_lowercase().next().unwrap());
                result.push_str(s.get(1..).unwrap());
                Ok(result)
            } else {
                Ok(s)
            }
        } else {
            Ok(s)
        }
    }

    fn uppercase_pattern(
        &mut self,
        s: &str,
        pattern: &Option<patterns::Pattern>,
    ) -> Result<String, error::Error> {
        if let Some(pattern) = pattern {
            if !pattern.is_empty() {
                let regex =
                    pattern.to_regex(false, false, self.parser_options.enable_extended_globbing)?;
                let result = regex.replace_all(s.as_ref(), |caps: &fancy_regex::Captures| {
                    caps[0].to_uppercase()
                });
                Ok(result.into_owned())
            } else {
                Ok(s.to_uppercase())
            }
        } else {
            Ok(s.to_uppercase())
        }
    }

    fn lowercase_pattern(
        &mut self,
        s: &str,
        pattern: &Option<patterns::Pattern>,
    ) -> Result<String, error::Error> {
        if let Some(pattern) = pattern {
            if !pattern.is_empty() {
                let regex =
                    pattern.to_regex(false, false, self.parser_options.enable_extended_globbing)?;
                let result = regex.replace_all(s.as_ref(), |caps: &fancy_regex::Captures| {
                    caps[0].to_lowercase()
                });
                Ok(result.into_owned())
            } else {
                Ok(s.to_lowercase())
            }
        } else {
            Ok(s.to_lowercase())
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn replace_substring(
        s: &str,
        regex: &fancy_regex::Regex,
        replacement: &str,
        match_kind: &SubstringMatchKind,
    ) -> Result<String, error::Error> {
        match match_kind {
            brush_parser::word::SubstringMatchKind::Prefix
            | brush_parser::word::SubstringMatchKind::Suffix
            | brush_parser::word::SubstringMatchKind::FirstOccurrence => {
                Ok(regex.replace(s, replacement).into_owned())
            }

            brush_parser::word::SubstringMatchKind::Anywhere => {
                Ok(regex.replace_all(s, replacement).into_owned())
            }
        }
    }

    fn apply_transform_to(
        &self,
        op: &ParameterTransformOp,
        s: &str,
    ) -> Result<String, error::Error> {
        match op {
            brush_parser::word::ParameterTransformOp::PromptExpand => {
                prompt::expand_prompt(self.shell, s)
            }
            brush_parser::word::ParameterTransformOp::CapitalizeInitial => {
                Ok(to_initial_capitals(s))
            }
            brush_parser::word::ParameterTransformOp::ExpandEscapeSequences => {
                let (result, _) =
                    escape::expand_backslash_escapes(s, escape::EscapeExpansionMode::AnsiCQuotes)?;
                Ok(String::from_utf8_lossy(result.as_slice()).into_owned())
            }
            brush_parser::word::ParameterTransformOp::PossiblyQuoteWithArraysExpanded {
                separate_words: _separate_words,
            } => {
                // TODO: This isn't right for arrays.
                // TODO: This doesn't honor 'separate_words'
                Ok(variables::quote_str_for_assignment(s))
            }
            brush_parser::word::ParameterTransformOp::Quoted => {
                Ok(variables::quote_str_for_assignment(s))
            }
            brush_parser::word::ParameterTransformOp::ToLowerCase => Ok(s.to_lowercase()),
            brush_parser::word::ParameterTransformOp::ToUpperCase => Ok(s.to_uppercase()),
            brush_parser::word::ParameterTransformOp::ToAssignmentLogic
            | brush_parser::word::ParameterTransformOp::ToAttributeFlags => {
                unreachable!("covered in caller")
            }
        }
    }
}

fn coalesce_expansions(expansions: Vec<Expansion>) -> Expansion {
    expansions
        .into_iter()
        .fold(Expansion::default(), |mut acc, expansion| {
            for (i, mut field) in expansion.fields.into_iter().enumerate() {
                match acc.fields.last_mut() {
                    Some(last) if i == 0 => {
                        last.0.append(&mut field.0);
                    }
                    _ => acc.fields.push(field),
                }
            }

            // TODO: What if expansions have different concatenation values?
            acc.concatenate = expansion.concatenate;

            acc
        })
}

fn to_initial_capitals(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in s.chars() {
        if c.is_whitespace() {
            capitalize_next = true;
            result.push(c);
        } else if capitalize_next {
            result.push_str(c.to_uppercase().to_string().as_str());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

fn valid_variable_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        Some(_) | None => false,
    }
}

fn transform_expansion(
    expansion: Expansion,
    mut f: impl FnMut(String) -> Result<String, error::Error>,
) -> Result<Expansion, error::Error> {
    let mut transformed_fields = vec![];
    for field in expansion.fields {
        transformed_fields.push(WordField::from(f(String::from(field))?));
    }

    Ok(Expansion {
        fields: transformed_fields,
        concatenate: expansion.concatenate,
        undefined: expansion.undefined,
    })
}

#[allow(clippy::panic_in_result_fn)]
#[allow(clippy::needless_return)]
#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[tokio::test]
    async fn test_full_expansion() -> Result<()> {
        let options = crate::shell::CreateOptions::default();
        let mut shell = crate::shell::Shell::new(&options).await?;

        assert_eq!(
            full_expand_and_split_str(&mut shell, "\"\"").await?,
            vec![""]
        );
        assert_eq!(
            full_expand_and_split_str(&mut shell, "a b").await?,
            vec!["a", "b"]
        );
        assert_eq!(
            full_expand_and_split_str(&mut shell, "ab").await?,
            vec!["ab"]
        );
        assert_eq!(
            full_expand_and_split_str(&mut shell, r#""a b""#).await?,
            vec!["a b"]
        );
        assert_eq!(
            full_expand_and_split_str(&mut shell, "").await?,
            Vec::<String>::new()
        );
        assert_eq!(
            full_expand_and_split_str(&mut shell, "$@").await?,
            Vec::<String>::new()
        );
        assert_eq!(
            full_expand_and_split_str(&mut shell, "$*").await?,
            Vec::<String>::new()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_field_splitting() -> Result<()> {
        let options = crate::shell::CreateOptions::default();
        let mut shell = crate::shell::Shell::new(&options).await?;
        let expander = WordExpander::new(&mut shell);

        let expansion = Expansion {
            fields: vec![
                WordField(vec![ExpansionPiece::Unsplittable("A".into())]),
                WordField(vec![ExpansionPiece::Unsplittable(String::new())]),
            ],
            ..Expansion::default()
        };

        let fields = expander.split_fields(expansion);

        assert_eq!(
            fields,
            vec![
                WordField(vec![ExpansionPiece::Unsplittable(String::from("A"))]),
                WordField(vec![ExpansionPiece::Unsplittable(String::new())])
            ]
        );

        Ok(())
    }

    #[test]
    fn test_to_initial_capitals() {
        assert_eq!(to_initial_capitals("ab bc cd"), String::from("Ab Bc Cd"));
        assert_eq!(to_initial_capitals(" a "), String::from(" A "));
        assert_eq!(to_initial_capitals(""), String::new());
    }

    #[test]
    fn test_valid_variable_name() {
        assert!(!valid_variable_name(""));
        assert!(!valid_variable_name("1"));
        assert!(!valid_variable_name(" a"));
        assert!(!valid_variable_name(" "));

        assert!(valid_variable_name("_"));
        assert!(valid_variable_name("_a"));
        assert!(valid_variable_name("_1"));
        assert!(valid_variable_name("_a1"));
        assert!(valid_variable_name("a"));
        assert!(valid_variable_name("A"));
        assert!(valid_variable_name("a1"));
        assert!(valid_variable_name("A1"));
    }
}
