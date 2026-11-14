//! Word expansion utilities.

use std::borrow::Cow;
use std::cmp::min;
use std::io::Write as _;

use brush_parser::word::{ParameterTransformOp, SubstringMatchKind};
use itertools::Itertools;

use crate::ExecutionParameters;
use crate::arithmetic;
use crate::arithmetic::ExpandAndEvaluate;
use crate::braceexpansion;
use crate::commands;
use crate::env;
use crate::error;
use crate::escape;
use crate::extensions;
use crate::patterns;
use crate::prompt;
use crate::shell::Shell;
use crate::sys;
use crate::trace_categories;
use crate::variables::ShellValueUnsetType;
use crate::variables::ShellVariable;
use crate::variables::{self, ShellValue};

mod fieldsplit;

/// Controls how the expander handles a backslash-escape sequence (`\X`)
/// when it appears outside any explicit quoting (single, double, ANSI-C).
/// Inside actual double-quoted text, the parser's own escape rules apply
/// and this setting is ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum UnquotedBackslashHandling {
    /// Default unquoted shell semantics: the backslash is consumed and `X`
    /// is preserved literally.
    #[default]
    Strip,
    /// The backslash and `X` are both preserved verbatim. Useful in pattern
    /// contexts where the matcher will interpret the backslash itself.
    Preserve,
}

/// The kind of word an expander is asked to expand. It decides whether braces are
/// expanded, where a tilde is expanded, and what an unquoted backslash does; no caller
/// sets those by hand.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum WordKind {
    /// A word of a command: braces are expanded and a leading tilde expands. With full
    /// expansion, the result is also split into fields and globbed.
    #[default]
    Command,
    /// A word that yields one string: a `[[ ]]` operand, a `case` subject, a
    /// redirection target, a subscript, the body of a here doc or a prompt. Bash
    /// performs no brace expansion on such a word.
    Scalar,
    /// The value of an assignment: as a scalar, except that a tilde after a colon also
    /// expands.
    AssignmentValue,
    /// A pattern as written in `[[ ]]` or `case`: as a scalar, except that an unquoted
    /// backslash is kept, with what it escapes, for the matcher.
    Pattern,
    /// An arithmetic expression: no tilde is expanded.
    ArithmeticExpression,
}

impl WordKind {
    /// Whether brace expansion applies to a word of this kind.
    const fn brace_expands(self) -> bool {
        matches!(self, Self::Command)
    }

    /// What to do with `\X` in unquoted text of a word of this kind.
    const fn unquoted_backslashes(self) -> UnquotedBackslashHandling {
        match self {
            Self::Pattern => UnquotedBackslashHandling::Preserve,
            _ => UnquotedBackslashHandling::Strip,
        }
    }
}

/// Options to customize the behavior of the word expander.
pub(crate) struct ExpanderOptions {
    /// The kind of word being expanded.
    pub kind: WordKind,
    /// Whether to perform command substitutions. If disabled, command substitutions
    /// are replaced with an empty string.
    pub execute_command_substitutions: bool,
    /// Whether to perform pathname expansion (globbing). If disabled, glob patterns
    /// are returned as literal strings.
    pub pathname_expand: bool,
}

impl Default for ExpanderOptions {
    fn default() -> Self {
        Self {
            kind: WordKind::default(),
            execute_command_substitutions: true,
            pathname_expand: true,
        }
    }
}

/// What kind of text a piece of a word is. It decides what quotes, backslashes and
/// tildes mean in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quoting {
    /// An ordinary word: quotes quote, a backslash escapes, a leading tilde expands.
    Unquoted,
    /// Double-quoted text, between double quotes or in a here doc body or a prompt whose
    /// quotes are elsewhere: only `$`, `` ` `` and `\` are special.
    DoubleQuoted,
}

/// How the text being expanded is to be read. It is handed down as the expander
/// descends into a word, not kept on the expander, so that each nested word gets the
/// context that is right for it: the word of `${param:-word}` is read in the context of
/// the expansion it stands in for, while a pattern or a replacement is a word of its own
/// and starts from [`WordContext::ordinary`].
#[derive(Clone, Copy, Debug)]
struct WordContext {
    /// The kind of text.
    quoting: Quoting,
    /// What to do with `\X` in unquoted text.
    unquoted_backslashes: UnquotedBackslashHandling,
    /// Whether the text starts a word, so that a tilde at its start is expanded.
    starts_word: bool,
    /// Whether a tilde after a colon is expanded too, as in the value of an assignment.
    tilde_after_colon: bool,
    /// Whether the text is within a here doc body or a prompt, at any depth. Within one,
    /// `$'...'` in the word of a double-quoted `${param:-word}` is text, even in a word
    /// of its own.
    within_body: bool,
}

impl WordContext {
    /// An ordinary word that stands on its own.
    const fn ordinary() -> Self {
        Self {
            quoting: Quoting::Unquoted,
            unquoted_backslashes: UnquotedBackslashHandling::Strip,
            starts_word: true,
            tilde_after_colon: false,
            within_body: false,
        }
    }

    /// The text of a here doc body or a prompt.
    const fn body() -> Self {
        Self {
            quoting: Quoting::DoubleQuoted,
            within_body: true,
            ..Self::ordinary()
        }
    }

    /// A word of its own nested in this text: ordinary, except that it remembers
    /// whether it is within a here doc body or a prompt.
    const fn own_word(self) -> Self {
        Self {
            within_body: self.within_body,
            ..Self::ordinary()
        }
    }

    /// The same context for text of another kind.
    const fn quoted_as(self, quoting: Quoting) -> Self {
        Self { quoting, ..self }
    }

    /// The same context for text that starts a word.
    const fn at_word_start(self) -> Self {
        Self {
            starts_word: true,
            ..self
        }
    }

    /// The same context for text that continues a word already begun.
    const fn mid_word(self) -> Self {
        Self {
            starts_word: false,
            ..self
        }
    }
}

/// Describes what a parameter's expansion is a list of. Operators whose behavior depends
/// on that shape -- `${#x}` and `${x:offset:length}` today -- consult this to decide
/// whether they address characters or elements, via the methods below.
#[derive(Clone, Debug)]
enum ExpansionKind {
    /// A single string; addressed by character.
    String,
    /// The elements of an indexed array, along with their subscripts in ascending order.
    /// The subscripts matter because bash addresses these by subscript rather than by
    /// position, which only makes a difference when the array is sparse.
    IndexedArray(Vec<u64>),
    /// Elements with no indices to address them by: an associative array, the positional
    /// parameters, or a generated list of names.
    ElementList,
    /// A scalar that was written `${x[@]}` or `${x[*]}`. bash counts it as a single
    /// element but slices it as a string.
    ScalarAsArray,
}

impl ExpansionKind {
    /// Returns whether `${x:offset:length}` rejects a negative length, which bash does for
    /// every real array form.
    const fn is_array(&self) -> bool {
        matches!(self, Self::IndexedArray(_) | Self::ElementList)
    }

    /// Returns whether `${#x}` counts characters rather than elements.
    const fn counts_chars(&self) -> bool {
        matches!(self, Self::String)
    }

    /// Returns whether `${x:offset:length}` addresses characters rather than elements.
    /// bash slices a scalar as a string even when it was written `${x[@]}`, so this is not
    /// the same question as [`Self::counts_chars`].
    const fn slices_chars(&self) -> bool {
        matches!(self, Self::String | Self::ScalarAsArray)
    }

    /// Returns the size of the space that `${x:offset:length}` offsets live in, which a
    /// negative offset counts back from. For an indexed array that is subscript space, one
    /// past the highest subscript; a sparse array's is wider than its element count.
    fn offset_space(&self, len: i64) -> i64 {
        match self {
            Self::IndexedArray(subscripts) => subscripts.last().map_or(0, |&last| {
                i64::try_from(last).unwrap_or(i64::MAX).saturating_add(1)
            }),
            _ => len,
        }
    }

    /// Returns the lowest offset that is out of range. bash validates the offset before it
    /// ever looks at the length, and yields an empty slice for anything at or past this
    /// point.
    fn offset_limit(&self, len: i64) -> i64 {
        match self {
            // An offset of exactly the length is in range for a string, and for elements
            // addressed by position; it just selects an empty slice.
            Self::String | Self::ScalarAsArray | Self::ElementList => len + 1,
            // An indexed array runs out just past its highest subscript.
            Self::IndexedArray(_) => self.offset_space(len),
        }
    }

    /// Translates an in-range offset into a position in the element list. For an indexed
    /// array bash selects the first element whose subscript is at least the offset; the
    /// length then counts elements positionally from there.
    fn offset_to_position(&self, offset: i64) -> i64 {
        match self {
            Self::IndexedArray(subscripts) => {
                let position =
                    subscripts.partition_point(|&s| i64::try_from(s).is_ok_and(|s| s < offset));
                i64::try_from(position).unwrap_or(i64::MAX)
            }
            _ => offset,
        }
    }
}

#[derive(Debug)]
struct Expansion {
    fields: Vec<WordField>,
    concatenate: bool,
    kind: ExpansionKind,
    undefined: bool,
}

impl Default for Expansion {
    fn default() -> Self {
        Self {
            fields: vec![],
            concatenate: true,
            kind: ExpansionKind::String,
            undefined: false,
        }
    }
}

impl From<String> for Expansion {
    fn from(value: String) -> Self {
        Self {
            fields: vec![WordField::from(value)],
            ..Self::default()
        }
    }
}

impl From<ExpansionPiece> for Expansion {
    fn from(piece: ExpansionPiece) -> Self {
        Self {
            fields: vec![WordField::from(piece)],
            ..Self::default()
        }
    }
}

impl Expansion {
    fn classify(&self) -> ParameterState {
        let non_empty = self
            .fields
            .iter()
            .any(|field| field.0.iter().any(|piece| !piece.as_str().is_empty()));

        if self.undefined {
            ParameterState::Undefined
        } else if non_empty {
            ParameterState::NonZeroLength
        } else if self.fields.is_empty() {
            // An array referenced via [@]/[*] that yields no fields (i.e. an empty
            // array) is treated as unset by bash for the +/- conditional operators.
            // This is distinct from a scalar holding an empty string, which has a
            // single (empty) field and remains a defined empty string.
            ParameterState::Undefined
        } else {
            ParameterState::DefinedEmptyString
        }
    }

    fn undefined() -> Self {
        Self {
            fields: vec![WordField::from(String::new())],
            concatenate: true,
            undefined: true,
            kind: ExpansionKind::String,
        }
    }

    /// Returns the length that `${#x}` reports.
    fn polymorphic_len(&self) -> usize {
        self.len_in(self.kind.counts_chars())
    }

    /// Returns the length that `${x:offset:length}` addresses.
    fn slice_len(&self) -> usize {
        self.len_in(self.kind.slices_chars())
    }

    fn len_in(&self, chars: bool) -> usize {
        if chars {
            self.fields.iter().fold(0, |acc, field| acc + field.len())
        } else {
            self.fields.len()
        }
    }

    fn polymorphic_subslice(&self, index: usize, end: usize) -> Self {
        let len = end - index;

        // If we're addressing elements, then interpret `index` and `end` as indices
        // into the elements.
        if !self.kind.slices_chars() {
            let actual_len = min(len, self.fields.len() - index);
            let fields = self.fields[index..(index + actual_len)].to_vec();

            Self {
                fields,
                concatenate: self.concatenate,
                undefined: self.undefined,
                kind: self.kind.clone(),
            }
        } else {
            // Otherwise, interpret `index` and `end` as indices into the string contents.
            let mut fields = vec![];

            // Keep track of how far away the interesting data is from the current read offset.
            let mut dist_to_slice = index;
            // Keep track of how many characters are left to be copied.
            let mut left = len;

            // Go through fields, copying the interesting parts.
            for field in &self.fields {
                let mut pieces = vec![];

                for piece in &field.0 {
                    // Stop once we've extracted enough characters.
                    if left == 0 {
                        break;
                    }

                    // Get the inner string of the piece, and figure out how many
                    // characters are in it; make sure to get the *character count*
                    // and not just call `.len()` to get the byte count.
                    let piece_str = piece.as_str();
                    let piece_char_count = piece_str.chars().count();

                    // If the interesting data isn't even in this piece yet, then
                    // continue until we find it.
                    if dist_to_slice >= piece_char_count {
                        dist_to_slice -= piece_char_count;
                        continue;
                    }

                    // Figure out how far into this piece we're interested in copying.
                    let desired_offset_into_this_piece = dist_to_slice;
                    // Figure out how many characters we're going to use from *this* piece.
                    let len_from_this_piece =
                        min(left, piece_char_count - desired_offset_into_this_piece);

                    let slice = |s: &str| -> String {
                        s.chars()
                            .skip(desired_offset_into_this_piece)
                            .take(len_from_this_piece)
                            .collect()
                    };
                    let new_piece = match piece {
                        ExpansionPiece::Unsplittable(s) => ExpansionPiece::Unsplittable(slice(s)),
                        ExpansionPiece::Splittable(s) => ExpansionPiece::Splittable(slice(s)),
                        ExpansionPiece::UnquotedLiteral(s) => {
                            ExpansionPiece::UnquotedLiteral(slice(s))
                        }
                        ExpansionPiece::EmptyListElement { has_following } => {
                            ExpansionPiece::EmptyListElement {
                                has_following: *has_following,
                            }
                        }
                    };

                    pieces.push(new_piece);

                    left -= len_from_this_piece;
                    dist_to_slice = 0;
                }

                if !pieces.is_empty() {
                    fields.push(WordField(pieces));
                }
            }

            Self {
                fields,
                concatenate: self.concatenate,
                undefined: self.undefined,
                kind: self.kind.clone(),
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct WordField(Vec<ExpansionPiece>);

impl WordField {
    pub const fn new() -> Self {
        Self(vec![])
    }

    pub fn len(&self) -> usize {
        self.0.iter().fold(0, |acc, piece| acc + piece.len())
    }
}

impl From<WordField> for String {
    fn from(field: WordField) -> Self {
        field.0.into_iter().map(Self::from).collect()
    }
}

impl From<WordField> for patterns::Pattern {
    fn from(value: WordField) -> Self {
        let pieces: Vec<_> = value
            .0
            .into_iter()
            .map(patterns::PatternPiece::from)
            .collect();

        Self::from(pieces)
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
    /// Quoted text: never field-split, never treated as a pattern.
    Unsplittable(String),
    /// The unquoted result of a parameter expansion, command substitution or arithmetic
    /// expansion: field-split on IFS, then treated as a pattern.
    Splittable(String),
    /// Unquoted literal text of the word itself: never field-split (bash only splits the
    /// results of expansions), but still treated as a pattern.
    UnquotedLiteral(String),
    /// An empty element of an unquoted array or positional-parameter expansion.
    EmptyListElement {
        /// Whether another element follows in the same list expansion.
        has_following: bool,
    },
}

impl From<ExpansionPiece> for String {
    fn from(piece: ExpansionPiece) -> Self {
        match piece {
            ExpansionPiece::Unsplittable(s)
            | ExpansionPiece::Splittable(s)
            | ExpansionPiece::UnquotedLiteral(s) => s,
            ExpansionPiece::EmptyListElement { .. } => Self::new(),
        }
    }
}

impl From<ExpansionPiece> for patterns::PatternPiece {
    fn from(piece: ExpansionPiece) -> Self {
        match piece {
            ExpansionPiece::Unsplittable(s) => Self::Literal(s),
            ExpansionPiece::Splittable(s) | ExpansionPiece::UnquotedLiteral(s) => Self::Pattern(s),
            ExpansionPiece::EmptyListElement { .. } => Self::Literal(String::new()),
        }
    }
}

impl From<ExpansionPiece> for crate::regex::RegexPiece {
    fn from(piece: ExpansionPiece) -> Self {
        match piece {
            ExpansionPiece::Unsplittable(s) => Self::Literal(s),
            ExpansionPiece::Splittable(s) | ExpansionPiece::UnquotedLiteral(s) => Self::Pattern(s),
            ExpansionPiece::EmptyListElement { .. } => Self::Literal(String::new()),
        }
    }
}

impl ExpansionPiece {
    const fn as_str(&self) -> &str {
        match self {
            Self::Unsplittable(s) | Self::Splittable(s) | Self::UnquotedLiteral(s) => s.as_str(),
            Self::EmptyListElement { .. } => "",
        }
    }

    const fn len(&self) -> usize {
        self.as_str().len()
    }

    fn make_unsplittable(self) -> Self {
        match self {
            Self::Unsplittable(_) => self,
            Self::Splittable(s) | Self::UnquotedLiteral(s) => Self::Unsplittable(s),
            Self::EmptyListElement { .. } => Self::Unsplittable(String::new()),
        }
    }

    fn list_element(value: String, has_following: bool) -> Self {
        if value.is_empty() {
            Self::EmptyListElement { has_following }
        } else {
            Self::Splittable(value)
        }
    }
}

enum ParameterState {
    Undefined,
    DefinedEmptyString,
    NonZeroLength,
}

/// Applies all basic expansion to the given word, yielding a pattern.
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The word to expand, as a string.
pub(crate) async fn basic_expand_pattern(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
) -> Result<patterns::Pattern, error::Error> {
    let mut expander = WordExpander::new(shell, params, WordKind::Pattern);

    let context = expander.top_level_context();
    expander
        .basic_expand_pattern(word_str.as_ref(), context)
        .await
}

/// Applies all basic expansion to the given word, yielding a regex.
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The word to expand, as a string.
pub(crate) async fn basic_expand_regex(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
) -> Result<crate::regex::Regex, error::Error> {
    let mut expander = WordExpander::new(shell, params, WordKind::Scalar);
    expander.basic_expand_regex(word_str.as_ref()).await
}

/// Applies all basic expansion to the given word (represented as a string).
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The word to expand, as a string.
pub(crate) async fn basic_expand_word(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
) -> Result<String, error::Error> {
    let mut expander = WordExpander::new(shell, params, WordKind::Scalar);
    expander.basic_expand_to_str(word_str.as_ref()).await
}

/// Expands a heredoc body.
///
/// Performs parameter expansion, command substitution, and arithmetic expansion
/// on the heredoc content while preserving literal quote characters. Unlike
/// [`basic_expand_word`], this treats `"` and `'` as literal characters rather
/// than quote delimiters.
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The heredoc body to expand, as a string.
pub(crate) async fn basic_expand_heredoc_word(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
) -> Result<String, error::Error> {
    expand_double_quoted_body(
        shell,
        params,
        word_str.as_ref(),
        brush_parser::word::DoubleQuotedTextKind::HereDoc,
    )
    .await
}

/// Expands a prompt string whose prompt escapes (`\w` and friends) have already been
/// decoded.
///
/// The text is expanded as a heredoc body is, except that a backslash is also consumed
/// before `"` and before a newline (a line continuation that leaves nothing behind).
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The prompt text to expand, as a string.
pub(crate) async fn basic_expand_prompt_word(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
) -> Result<String, error::Error> {
    expand_double_quoted_body(
        shell,
        params,
        word_str.as_ref(),
        brush_parser::word::DoubleQuotedTextKind::Prompt,
    )
    .await
}

/// Expands the body of a double-quoted string; see
/// [`WordExpander::expand_double_quoted_body`].
async fn expand_double_quoted_body(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    body: &str,
    kind: brush_parser::word::DoubleQuotedTextKind,
) -> Result<String, error::Error> {
    // A body is a scalar word: braces aren't expanded anywhere within double quotes,
    // words nested in the body included.
    let mut expander = WordExpander::new(shell, params, WordKind::Scalar);
    expander.expand_double_quoted_body(body, kind).await
}

/// Applies all basic expansion to the given word (represented as a string),
/// with custom expander options.
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The word to expand, as a string.
pub(crate) async fn basic_expand_word_with_options(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
    options: &ExpanderOptions,
) -> Result<String, error::Error> {
    let mut expander = WordExpander::new_from_options(shell, params, options);
    expander.basic_expand_to_str(word_str.as_ref()).await
}

/// Apply tilde-expansion, parameter expansion, command substitution, and arithmetic expansion;
/// then perform field splitting and pathname expansion on the result.
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The word to expand, as a string.
pub(crate) async fn full_expand_and_split_word(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
) -> Result<Vec<String>, error::Error> {
    let mut expander = WordExpander::new(shell, params, WordKind::Command);
    expander.full_expand_with_splitting(word_str.as_ref()).await
}

/// Apply tilde-expansion, parameter expansion, command substitution, and arithmetic expansion;
/// then perform field splitting and pathname expansion on the result.
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The word to expand, as a string.
/// * `options` - Options to customize the behavior of the expander.
pub(crate) async fn full_expand_and_split_word_with_options(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
    options: &ExpanderOptions,
) -> Result<Vec<String>, error::Error> {
    let mut expander = WordExpander::new_from_options(shell, params, options);
    expander.full_expand_with_splitting(word_str.as_ref()).await
}

/// Expands a word in assignment context (enables tilde-after-colon expansion).
///
/// # Arguments
///
/// * `shell` - The shell in which to perform expansion.
/// * `params` - The execution parameters to use during expansion.
/// * `word_str` - The word to expand, as a string.
pub(crate) async fn basic_expand_assignment_word(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    word_str: impl AsRef<str>,
) -> Result<String, error::Error> {
    let mut expander = WordExpander::new(shell, params, WordKind::AssignmentValue);
    expander.basic_expand_to_str(word_str.as_ref()).await
}

/// Assigns a value to a named parameter.
///
/// # Arguments
///
/// * `shell` - The shell in which to perform the assignment.
/// * `params` - The execution parameters to use during the assignment.
/// * `name` - The name of the parameter to assign to. May be a variable name, or a more complex,
///   assignable parameter expression (e.g., an array element).
/// * `value` - The value to assign to the parameter.
pub async fn assign_to_named_parameter(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    name: &str,
    value: String,
) -> Result<(), error::Error> {
    let parser_options = shell.parser_options();
    let mut expander = WordExpander::new(shell, params, WordKind::Scalar);
    let parameter = brush_parser::word::parse_parameter(name, &parser_options)?;
    let context = expander.top_level_context();
    expander
        .assign_to_parameter(&parameter, value, context)
        .await
}

struct WordExpander<'a, SE: extensions::ShellExtensions> {
    /// The shell in which to perform expansion.
    shell: &'a mut Shell<SE>,
    /// The execution parameters to use during expansion.
    params: &'a ExecutionParameters,
    /// The parser options to use during expansion.
    parser_options: brush_parser::ParserOptions,
    /// The kind of word being expanded.
    kind: WordKind,
    /// Whether to disable command substitutions.
    disable_command_substitutions: bool,
    /// Whether to disable pathname expansion (globbing).
    disable_pathname_expansion: bool,
}

impl<'a, SE: extensions::ShellExtensions> WordExpander<'a, SE> {
    /// Creates an expander for a word of the given kind.
    pub const fn new(
        shell: &'a mut Shell<SE>,
        params: &'a ExecutionParameters,
        kind: WordKind,
    ) -> Self {
        Self::new_from_options(
            shell,
            params,
            &ExpanderOptions {
                kind,
                execute_command_substitutions: true,
                pathname_expand: true,
            },
        )
    }

    pub const fn new_from_options(
        shell: &'a mut Shell<SE>,
        params: &'a ExecutionParameters,
        options: &ExpanderOptions,
    ) -> Self {
        let mut parser_options = shell.parser_options();

        // An arithmetic expression expands no tilde at all. Where the other kinds expand
        // one is for the context to say, word by word.
        if matches!(options.kind, WordKind::ArithmeticExpression) {
            parser_options.tilde_expansion_at_word_start = false;
            parser_options.tilde_expansion_after_colon = false;
        }

        Self {
            shell,
            params,
            parser_options,
            kind: options.kind,
            disable_command_substitutions: !options.execute_command_substitutions,
            disable_pathname_expansion: !options.pathname_expand,
        }
    }

    /// The context of the word this expander was asked to expand.
    const fn top_level_context(&self) -> WordContext {
        WordContext {
            unquoted_backslashes: self.kind.unquoted_backslashes(),
            tilde_after_colon: matches!(self.kind, WordKind::AssignmentValue),
            ..WordContext::ordinary()
        }
    }

    /// Apply tilde-expansion, parameter expansion, command substitution, and arithmetic expansion.
    pub async fn basic_expand_to_str(&mut self, word: &str) -> Result<String, error::Error> {
        let expansion = self.expand_word(word).await?;
        Ok(self.fields_to_string(expansion))
    }

    /// Join an [`Expansion`]'s fields into a single string for scalar
    /// (non-field-splitting) contexts, following bash rules: `$*`/`${a[*]}`
    /// (`concatenate=true`) join with the first character of IFS, while
    /// `$@`/`${a[@]}` join with a space. A single field (the common case) is
    /// unaffected. Without this, e.g. `x=${arr[*]}` under `IFS=:` would yield
    /// `a b c` instead of bash's `a:b:c`.
    fn fields_to_string(&self, expansion: Expansion) -> String {
        let joiner = if expansion.concatenate {
            self.shell.ifs_joiner()
        } else {
            String::from(' ')
        };
        expansion
            .fields
            .into_iter()
            .map(String::from)
            .join(joiner.as_str())
    }

    /// Expand a word that is part of a parameter expansion and is a word of its own
    /// whatever encloses that expansion: a pattern, a replacement, an error message, a
    /// subscript. Neither the quotes around the expansion nor the caller's handling of
    /// unquoted backslashes reach into it.
    ///
    /// # Arguments
    ///
    /// * `word` - The word to expand.
    /// * `context` - The context of the parameter expansion the word is part of.
    async fn expand_own_word(
        &mut self,
        word: &str,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        self.basic_expand(word, context.own_word()).await
    }

    /// As [`Self::expand_own_word`], joining the result into a string.
    async fn expand_own_word_to_str(
        &mut self,
        word: &str,
        context: WordContext,
    ) -> Result<String, error::Error> {
        let expansion = self.expand_own_word(word, context).await?;
        Ok(self.fields_to_string(expansion))
    }

    /// As [`Self::expand_own_word`], yielding a pattern if there is a word.
    async fn basic_expand_opt_pattern(
        &mut self,
        word: Option<&str>,
        context: WordContext,
    ) -> Result<Option<patterns::Pattern>, error::Error> {
        if let Some(word) = word {
            let pattern = self
                .basic_expand_pattern(word, context.own_word())
                .await?
                .set_extended_globbing(self.parser_options.enable_extended_globbing);

            Ok(Some(pattern))
        } else {
            Ok(None)
        }
    }

    async fn basic_expand_pattern(
        &mut self,
        word: &str,
        context: WordContext,
    ) -> Result<patterns::Pattern, error::Error> {
        let expansion = self.basic_expand(word, context).await?;
        Ok(Self::expansion_to_pattern(expansion))
    }

    fn expansion_to_pattern(expansion: Expansion) -> patterns::Pattern {
        // TODO(IFS): Use IFS instead for separator?
        #[expect(unstable_name_collisions)]
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

        patterns::Pattern::from(pattern_pieces)
    }

    async fn basic_expand_regex(
        &mut self,
        word: &str,
    ) -> Result<crate::regex::Regex, error::Error> {
        let expansion = self.expand_word(word).await?;

        // TODO(IFS): Use IFS instead for separator?
        #[expect(unstable_name_collisions)]
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

        Ok(crate::regex::Regex::from(regex_pieces)
            .set_case_insensitive(self.shell.options().case_insensitive_conditionals))
    }

    /// Expand the word this expander was built for. Brace expansion comes first, as
    /// bash performs it on the word as written before every other expansion; each
    /// result is then a word of its own, which is what keeps the results separate when
    /// IFS lacks a space. Only the word as a whole is brace-expanded, never a word
    /// nested in it.
    async fn expand_word(&mut self, word: &str) -> Result<Expansion, error::Error> {
        let context = self.top_level_context();

        let Some(brace_words) = self.brace_expand_if_needed(word) else {
            return self.basic_expand(word, context).await;
        };

        tracing::debug!(target: trace_categories::EXPANSION, "  => brace expanded to {brace_words:?}");

        // A single result (e.g. `{2..2}`) keeps the inner expansion's properties, such as
        // the `concatenate` flag of "${arr[@]}". With several results each one contributes
        // its fields and array semantics don't propagate.
        if let [only] = brace_words.as_slice() {
            return self.basic_expand(only, context).await;
        }

        let mut fields = vec![];
        for brace_word in &brace_words {
            fields.extend(self.basic_expand(brace_word, context).await?.fields);
        }

        Ok(Expansion {
            fields,
            ..Expansion::default()
        })
    }

    /// Apply tilde-expansion, parameter expansion, command substitution and arithmetic
    /// expansion to a word (the word as written, one result of brace-expanding it, or a
    /// word nested in another); yield pieces that could be further processed.
    async fn basic_expand(
        &mut self,
        word: &str,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        tracing::debug!(target: trace_categories::EXPANSION, "Basic expanding: '{word}'");

        // Quick short circuit to avoid more expensive parsing. The characters below are
        // understood to be the *only* ones indicative of *possible* expansion. There's
        // still a possibility no expansion needs to be done, but that's okay; we'll still
        // yield a correct result.
        if !word.contains(['$', '`', '\\', '\'', '\"', '~']) {
            return Ok(Expansion::from(ExpansionPiece::UnquotedLiteral(
                word.to_owned(),
            )));
        }

        let pieces = if context.starts_word && !context.tilde_after_colon {
            brush_parser::word::parse(word, &self.parser_options)?
        } else {
            // A tilde is only expanded at the start of a word, and after a colon only in
            // the value of an assignment.
            let mut options = self.parser_options.clone();
            options.tilde_expansion_at_word_start &= context.starts_word;
            options.tilde_expansion_after_colon = context.tilde_after_colon;
            brush_parser::word::parse(word, &options)?
        };

        let mut expansions = Vec::with_capacity(pieces.len());
        for piece in pieces {
            expansions.push(self.expand_word_piece(piece.piece, context).await?);
        }

        Ok(coalesce_expansions(expansions))
    }

    /// Expand the body of a double-quoted string whose quotes are elsewhere, such as a
    /// heredoc body or a prompt: `"` and `'` are ordinary characters, and a backslash
    /// escapes only `$`, `` ` `` and `\`. The word in `${param:-word}` and its like is
    /// more of the same double-quoted text (see [`Self::expand_inheriting_word`]); a
    /// pattern or a replacement (`${param#pattern}`, `${param/pattern/replacement}`) is
    /// a word of its own, tildes included (see [`Self::expand_own_word`]). Brace
    /// expansion doesn't happen within double quotes; a body's expander is built for a
    /// scalar word, which isn't brace-expanded.
    ///
    /// # Arguments
    ///
    /// * `body` - The text to expand.
    /// * `kind` - The kind of body: a prompt has `\"` stand for `"` and joins continued
    ///   lines, where a heredoc body keeps `\"` as written.
    async fn expand_double_quoted_body(
        &mut self,
        body: &str,
        kind: brush_parser::word::DoubleQuotedTextKind,
    ) -> Result<String, error::Error> {
        // Nothing else is special in such a body.
        if !body.contains(['$', '`', '\\']) {
            return Ok(body.to_owned());
        }

        let pieces =
            brush_parser::word::parse_double_quoted_text(body, kind, &self.parser_options)?;

        let context = WordContext::body();
        let expansion = self.expand_double_quoted_pieces(pieces, context).await?;

        Ok(self.fields_to_string(expansion))
    }

    /// Expand a word that stands in for the value of a parameter expansion (the word in
    /// `${param:-word}`, `${param:+word}` or `${param:=word}`). Unlike a pattern or a
    /// replacement (see [`Self::expand_own_word`]), it is read as the kind of text the
    /// expansion appears in.
    async fn expand_inheriting_word(
        &mut self,
        word: &str,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        let mut expansion = match context.quoting {
            // The word is more of the enclosing double-quoted text, whatever quotes it
            // holds: single quotes are ordinary characters, tildes aren't expanded, and
            // double quotes neither end the enclosing quotes nor show.
            Quoting::DoubleQuoted => {
                let kind = if context.within_body {
                    brush_parser::word::DoubleQuotedTextKind::WordInHereDocOrPrompt
                } else {
                    brush_parser::word::DoubleQuotedTextKind::WordInDoubleQuotes
                };
                let pieces =
                    brush_parser::word::parse_double_quoted_text(word, kind, &self.parser_options)?;

                self.expand_double_quoted_pieces(pieces, context).await?
            }
            // An unquoted word: its quotes quote and are removed.
            Quoting::Unquoted => self.basic_expand(word, context.at_word_start()).await?,
        };

        // The word becomes part of the parameter expansion's result, which bash
        // field-splits as a whole: `${x:-a:b}` under `IFS=:` is two fields.
        for field in &mut expansion.fields {
            for piece in &mut field.0 {
                if let ExpansionPiece::UnquotedLiteral(s) = piece {
                    *piece = ExpansionPiece::Splittable(std::mem::take(s));
                }
            }
        }

        Ok(expansion)
    }

    /// Performs brace expansion on the word, yielding the resulting words, or `None` if
    /// the word contains no brace expression.
    fn brace_expand_if_needed(&self, word: &str) -> Option<Vec<String>> {
        // We perform a non-authoritative check to see if the string *may* contain braces
        // to expand. There may be false positives, but must be no false negatives.
        if !self.kind.brace_expands()
            || !self.shell.options().perform_brace_expansion
            || !may_contain_braces_to_expand(word)
        {
            return None;
        }

        let brace_expansion_pieces =
            match brush_parser::word::parse_brace_expansions(word, &self.parser_options) {
                Ok(Some(pieces)) => pieces,
                Ok(None) => return None,
                Err(err) => {
                    tracing::error!("failed to parse for brace expansion: {err:?}");
                    return None;
                }
            };

        tracing::debug!(target: trace_categories::EXPANSION, "Brace expansion pieces: {brace_expansion_pieces:?}");

        // An alternative that expands to nothing yields no word at all (like any other
        // unquoted empty expansion); `{a,}` is just `a`.
        let words = braceexpansion::generate_and_combine_brace_expansions(brace_expansion_pieces)
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();

        Some(words)
    }

    /// Apply tilde-expansion, parameter expansion, command substitution, and arithmetic expansion;
    /// then perform field splitting and pathname expansion.
    pub async fn full_expand_with_splitting(
        &mut self,
        word: &str,
    ) -> Result<Vec<String>, error::Error> {
        // Perform basic expansion first.
        let basic_expansion = self.expand_word(word).await?;

        // Then split.
        let fields: Vec<WordField> = fieldsplit::split_fields(&self.shell.ifs(), basic_expansion);

        // Now expand pathnames if necessary. This also unquotes as a side effect.
        // We also know a length that the vector may be at minimally.
        let mut result = Vec::with_capacity(fields.len());
        for field in fields {
            if self.disable_pathname_expansion || self.shell.options().disable_filename_globbing {
                result.push(String::from(field));
            } else {
                result.extend(self.expand_pathnames_in_field(field)?);
            }
        }

        Ok(result)
    }

    fn expand_pathnames_in_field(&self, field: WordField) -> Result<Vec<String>, error::Error> {
        let pattern = patterns::Pattern::from(field.clone())
            .set_extended_globbing(self.parser_options.enable_extended_globbing)
            .set_case_insensitive(self.shell.options().case_insensitive_pathname_expansion);

        let options = patterns::FilenameExpansionOptions {
            require_dot_in_pattern_to_match_dot_files: !self.shell.options().glob_matches_dotfiles,
        };

        // On error (e.g. malformed pattern), default to NoGlob so the field
        // passes through as a literal rather than triggering failglob.
        let expansion = pattern
            .expand(
                self.shell.working_dir(),
                Some(&patterns::Pattern::accept_all_expand_filter),
                &options,
            )
            .unwrap_or_default();

        if expansion.is_unmatched_glob()
            && self.shell.options().fail_expansion_on_globs_without_match
        {
            let field_str = String::from(field);
            return Err(error::ErrorKind::NoMatch(field_str).into());
        }

        let paths = expansion.into_paths();
        if paths.is_empty() {
            if self.shell.options().expand_non_matching_patterns_to_null {
                Ok(vec![])
            } else {
                Ok(vec![String::from(field)])
            }
        } else {
            Ok(paths)
        }
    }

    #[allow(clippy::double_must_use)]
    #[async_recursion::async_recursion]
    async fn expand_word_piece(
        &mut self,
        word_piece: brush_parser::word::WordPiece,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        let expansion: Expansion = match word_piece {
            brush_parser::word::WordPiece::Text(s) => {
                Expansion::from(ExpansionPiece::UnquotedLiteral(s))
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
            brush_parser::word::WordPiece::DoubleQuotedSequence(pieces)
            | brush_parser::word::WordPiece::GettextDoubleQuotedSequence(pieces) => {
                self.expand_double_quoted_pieces(pieces, context.quoted_as(Quoting::DoubleQuoted))
                    .await?
            }
            brush_parser::word::WordPiece::TildeExpansion(tilde_expr) => {
                Expansion::from(ExpansionPiece::Unsplittable(
                    self.expand_tilde_expression(&tilde_expr)?.to_string(),
                ))
            }
            brush_parser::word::WordPiece::ParameterExpansion(p) => {
                self.expand_parameter_expr(p, context).await?
            }
            brush_parser::word::WordPiece::BackquotedCommandSubstitution(s)
            | brush_parser::word::WordPiece::CommandSubstitution(s) => {
                let mut cmd_output = if !self.disable_command_substitutions {
                    commands::invoke_command_in_subshell_and_get_output(self.shell, self.params, s)
                        .await?
                } else {
                    String::new()
                };

                // Strips null bytes from command substitution output for compatibility.
                if cmd_output.contains('\0') {
                    writeln!(
                        self.params.stderr(self.shell),
                        "warning: command substitution: ignored null byte in input",
                    )?;
                    cmd_output.retain(|c| c != '\0');
                }

                // We trim trailing newlines, per spec.
                let trimmed_len = cmd_output.trim_end_matches('\n').len();
                cmd_output.truncate(trimmed_len);

                Expansion::from(ExpansionPiece::Splittable(cmd_output))
            }
            brush_parser::word::WordPiece::EscapeSequence(s) => {
                let Some(escaped) = s.strip_prefix('\\') else {
                    // We don't ever expect this case, as it breaks our invariant--but
                    // we handle it to avoid panicking.
                    return Ok(Expansion::from(ExpansionPiece::Unsplittable(s)));
                };

                match (context.quoting, context.unquoted_backslashes) {
                    // Within double-quoted text, the parser only emits an EscapeSequence
                    // for `\X` where X is escapable there; the backslash always goes.
                    (Quoting::DoubleQuoted, _)
                    // Default unquoted semantics: consume the backslash.
                    | (Quoting::Unquoted, UnquotedBackslashHandling::Strip) => {
                        Expansion::from(ExpansionPiece::Unsplittable(escaped.to_owned()))
                    }
                    // Preserve `\X` verbatim (e.g., for pattern syntax).
                    (Quoting::Unquoted, UnquotedBackslashHandling::Preserve) => {
                        Expansion::from(ExpansionPiece::Splittable(s))
                    }
                }
            }
            brush_parser::word::WordPiece::ArithmeticExpression(e) => Expansion::from(
                ExpansionPiece::Splittable(self.expand_arithmetic_expr(e).await?),
            ),
        };

        Ok(expansion)
    }

    fn expand_tilde_expression(
        &self,
        tilde_expr: &brush_parser::word::TildeExpr,
    ) -> Result<Cow<'_, str>, error::Error> {
        match tilde_expr {
            brush_parser::word::TildeExpr::Home => {
                if let Some(home_dir) = self.shell.home_dir() {
                    Ok(Cow::Owned(home_dir.to_string_lossy().to_string()))
                } else {
                    Err(error::ErrorKind::TildeWithoutValidHome.into())
                }
            }
            brush_parser::word::TildeExpr::UserHome(username) => {
                Ok(sys::users::get_user_home_dir(username).map_or_else(
                    || Cow::Owned(std::format!("~{username}")),
                    |p| Cow::Owned(p.to_string_lossy().to_string()),
                ))
            }
            brush_parser::word::TildeExpr::WorkingDir => {
                Ok(self.shell.working_dir().to_string_lossy())
            }
            brush_parser::word::TildeExpr::OldWorkingDir => {
                // OLDPWD may be declared without a value (e.g. at startup); only
                // expand `~-` when it actually has one.
                Ok(self
                    .shell
                    .env_var("OLDPWD")
                    .filter(|var| var.value().is_set())
                    .map_or(Cow::Borrowed("~-"), |var| {
                        var.value().to_cow_str(self.shell)
                    }))
            }
            brush_parser::word::TildeExpr::NthDirFromBottomOfDirStack { n } => {
                let dir_stack_count = self.shell.directory_stack().len();

                if let Some(dir) = self.shell.directory_stack().get(*n) {
                    Ok(dir.to_string_lossy())
                } else if *n == dir_stack_count {
                    Ok(self.shell.working_dir().to_string_lossy())
                } else {
                    Ok(Cow::Owned(std::format!("~-{n}")))
                }
            }
            brush_parser::word::TildeExpr::NthDirFromTopOfDirStack { n, plus_used } => {
                if *n == 0 {
                    return Ok(self.shell.working_dir().to_string_lossy());
                }

                let dir_stack_count = self.shell.directory_stack().len();
                if dir_stack_count >= *n
                    && let Some(dir) = self.shell.directory_stack().get(dir_stack_count - *n)
                {
                    return Ok(dir.to_string_lossy());
                }

                let plus_or_nothing = if *plus_used { "+" } else { "" };
                Ok(Cow::Owned(std::format!("~{plus_or_nothing}{n}")))
            }
        }
    }

    /// Expand the pieces of double-quoted text. This ensures proper handling of
    /// concatenation and field building.
    ///
    /// # Arguments
    ///
    /// * `pieces` - The pieces to expand.
    /// * `context` - The context of the pieces; its kind of text is double-quoted.
    async fn expand_double_quoted_pieces(
        &mut self,
        pieces: Vec<brush_parser::word::WordPieceWithSource>,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        // Text with no pieces is still a word: an empty, unsplittable string.
        if pieces.is_empty() {
            return Ok(Expansion::from(ExpansionPiece::Unsplittable(String::new())));
        }

        let mut fields: Vec<WordField> = vec![];
        let concatenation_joiner = self.shell.ifs_joiner();

        for piece in pieces {
            let Expansion {
                fields: this_fields,
                concatenate,
                ..
            } = self.expand_word_piece(piece.piece, context).await?;

            let fields_to_append = if concatenate {
                #[expect(unstable_name_collisions)]
                let mut concatenated: Vec<ExpansionPiece> = this_fields
                    .into_iter()
                    .map(|WordField(pieces)| {
                        pieces
                            .into_iter()
                            .map(|piece| piece.make_unsplittable())
                            .collect()
                    })
                    .intersperse(vec![ExpansionPiece::Unsplittable(
                        concatenation_joiner.clone(),
                    )])
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

                if i == 0
                    && let Some(WordField(last_pieces)) = fields.last_mut()
                {
                    last_pieces.append(&mut next_pieces);
                    continue;
                }

                fields.push(WordField(next_pieces));
            }
        }

        Ok(Expansion {
            fields,
            concatenate: false,
            undefined: false,
            kind: ExpansionKind::String,
        })
    }

    #[expect(clippy::too_many_lines)]
    async fn expand_parameter_expr(
        &mut self,
        expr: brush_parser::word::ParameterExpr,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        #[expect(clippy::cast_possible_truncation)]
        match expr {
            brush_parser::word::ParameterExpr::Parameter {
                parameter,
                indirect,
            } => self.expand_parameter(&parameter, indirect, context).await,
            brush_parser::word::ParameterExpr::UseDefaultValues {
                parameter,
                indirect,
                test_type,
                default_value,
            } => {
                let expanded_parameter = self
                    .expand_parameter_allowing_unset(&parameter, indirect, context)
                    .await?;
                let default_value = default_value.as_ref().map_or("", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(expanded_parameter),
                    _ => Ok(self.expand_inheriting_word(default_value, context).await?),
                }
            }
            brush_parser::word::ParameterExpr::AssignDefaultValues {
                parameter,
                indirect,
                test_type,
                default_value,
            } => {
                let expanded_parameter = self
                    .expand_parameter_allowing_unset(&parameter, indirect, context)
                    .await?;
                let default_value = default_value.as_ref().map_or("", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(expanded_parameter),
                    _ => {
                        let expanded_default =
                            self.expand_inheriting_word(default_value, context).await?;
                        let expanded_default_value = self.fields_to_string(expanded_default);
                        self.assign_to_parameter(
                            &parameter,
                            expanded_default_value.clone(),
                            context,
                        )
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
                let expanded_parameter = self
                    .expand_parameter_allowing_unset(&parameter, indirect, context)
                    .await?;
                let error_message = error_message.as_ref().map_or("", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(expanded_parameter),
                    _ => {
                        let result = self.expand_own_word_to_str(error_message, context).await?;
                        let err: error::Error =
                            error::ErrorKind::CheckedExpansionError(result).into();

                        // Expansion errors are fatal per POSIX spec
                        Err(err.into_fatal())
                    }
                }
            }
            brush_parser::word::ParameterExpr::UseAlternativeValue {
                parameter,
                indirect,
                test_type,
                alternative_value,
            } => {
                let expanded_parameter = self
                    .expand_parameter_allowing_unset(&parameter, indirect, context)
                    .await?;
                let alternative_value = alternative_value.as_ref().map_or("", |v| v.as_str());

                match (test_type, expanded_parameter.classify()) {
                    (_, ParameterState::NonZeroLength)
                    | (
                        brush_parser::word::ParameterTestType::Unset,
                        ParameterState::DefinedEmptyString,
                    ) => Ok(self
                        .expand_inheriting_word(alternative_value, context)
                        .await?),
                    // "${arr[@]+word}" or "${@+word}" with no elements expands to nothing
                    // (zero words), just as "${arr[@]}" and "$@" do.
                    _ if !expanded_parameter.concatenate
                        && expanded_parameter.fields.is_empty() =>
                    {
                        Ok(expanded_parameter)
                    }
                    _ => Ok(Expansion::from(String::new())),
                }
            }
            brush_parser::word::ParameterExpr::ParameterLength {
                parameter,
                indirect,
            } => {
                // In bash, ${#arr[i]} returns 0 for unset elements of a
                // declared array even with `set -u`. But ${#unset_var} still
                // errors. Allow unset only for array element/all-indices
                // access on variables that exist.
                let allow_unset = match &parameter {
                    brush_parser::word::Parameter::NamedWithIndex { name, .. }
                    | brush_parser::word::Parameter::NamedWithAllIndices { name, .. } => {
                        self.shell.env().get(name).is_some()
                    }
                    _ => false,
                };
                let expansion = if allow_unset {
                    self.expand_parameter_allowing_unset(&parameter, indirect, context)
                        .await?
                } else {
                    self.expand_parameter(&parameter, indirect, context).await?
                };
                Ok(Expansion::from(expansion.polymorphic_len().to_string()))
            }
            brush_parser::word::ParameterExpr::RemoveSmallestSuffixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;
                transform_expansion(expanded_parameter, async |s| {
                    patterns::remove_smallest_matching_suffix(s.as_str(), expanded_pattern.as_ref())
                        .map(|s| s.to_owned())
                })
                .await
            }
            brush_parser::word::ParameterExpr::RemoveLargestSuffixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;
                transform_expansion(expanded_parameter, async |s| {
                    patterns::remove_largest_matching_suffix(s.as_str(), expanded_pattern.as_ref())
                        .map(|s| s.to_owned())
                })
                .await
            }
            brush_parser::word::ParameterExpr::RemoveSmallestPrefixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;

                transform_expansion(expanded_parameter, async |s| {
                    patterns::remove_smallest_matching_prefix(s.as_str(), expanded_pattern.as_ref())
                        .map(|s| s.to_owned())
                })
                .await
            }
            brush_parser::word::ParameterExpr::RemoveLargestPrefixPattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;

                transform_expansion(expanded_parameter, async |s| {
                    patterns::remove_largest_matching_prefix(s.as_str(), expanded_pattern.as_ref())
                        .map(|s| s.to_owned())
                })
                .await
            }
            brush_parser::word::ParameterExpr::Substring {
                parameter,
                indirect,
                offset,
                length,
            } => {
                let mut expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;

                // If this is ${@:...} then make sure $0 is in the array being sliced.
                if matches!(
                    parameter,
                    brush_parser::word::Parameter::Special(
                        brush_parser::word::SpecialParameter::AllPositionalParameters {
                            concatenate: _
                        },
                    )
                ) {
                    let shell_name = self.shell.current_shell_name().unwrap_or_else(|| "".into());

                    expanded_parameter.fields.insert(
                        0,
                        WordField::from(ExpansionPiece::Splittable(shell_name.to_string())),
                    );
                }

                #[expect(clippy::cast_possible_wrap)]
                let expanded_parameter_len = expanded_parameter.slice_len() as i64;

                // bash has nothing to slice when the parameter is unset, or when it
                // expanded to no fields at all -- an empty array, say -- and yields an
                // empty slice without validating the offset or the length.
                if matches!(expanded_parameter.classify(), ParameterState::Undefined) {
                    return Ok(expanded_parameter.polymorphic_subslice(0, 0));
                }

                // A negative offset counts back from the end of the offset space: -1
                // addresses the last character or position, or -- for an indexed array --
                // the highest subscript.
                let mut expanded_offset = offset.eval(self.shell, self.params, false).await?;
                if expanded_offset < 0 {
                    expanded_offset += expanded_parameter.kind.offset_space(expanded_parameter_len);
                }

                // bash validates the offset before it looks at the length: an offset that is
                // out of range -- still negative after counting back from the end, or at or
                // past the end -- yields an empty slice, whatever the length says.
                if expanded_offset < 0
                    || expanded_offset
                        >= expanded_parameter.kind.offset_limit(expanded_parameter_len)
                {
                    return Ok(expanded_parameter.polymorphic_subslice(0, 0));
                }

                // Offsets are validated in the space they were written in; from here on we
                // need a position in the element list.
                let expanded_offset = expanded_parameter.kind.offset_to_position(expanded_offset);

                let end_offset = if let Some(length) = length {
                    let expanded_length = length.eval(self.shell, self.params, false).await?;

                    if expanded_length < 0 {
                        // For a string, a negative length says where the slice *ends*,
                        // counting back from the end of the value, instead of how long it
                        // is: `${s:2:-2}` on `abcdefgh` ends at index 6, giving `cdef`. bash
                        // has no such reading for an array, and rejects a negative length
                        // there outright.
                        let end_offset = expanded_parameter_len + expanded_length;
                        if expanded_parameter.kind.is_array() || end_offset < expanded_offset {
                            return Err(error::ErrorKind::CheckedExpansionError(format!(
                                "{expanded_length}: substring expression < 0"
                            ))
                            .into());
                        }
                        end_offset
                    } else {
                        min(
                            expanded_offset.saturating_add(expanded_length),
                            expanded_parameter_len,
                        )
                    }
                } else {
                    expanded_parameter_len
                };

                #[expect(clippy::cast_sign_loss)]
                Ok(expanded_parameter
                    .polymorphic_subslice(expanded_offset as usize, end_offset as usize))
            }
            brush_parser::word::ParameterExpr::Transform {
                parameter,
                indirect,
                op: ParameterTransformOp::ToAttributeFlags,
            } => {
                if let (_, _, Some(var)) = self
                    .try_resolve_parameter_to_variable(&parameter, indirect, context)
                    .await?
                {
                    Ok(var.attribute_flags(self.shell).into())
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
                    .try_resolve_parameter_to_variable(&parameter, indirect, context)
                    .await?
                {
                    let assignable_value_str = var
                        .value()
                        .to_assignable_str(index.as_deref(), self.shell)?;

                    let mut attr_str = var.attribute_flags(self.shell);
                    if attr_str.is_empty() {
                        attr_str.push('-');
                    }

                    match var.value() {
                        ShellValue::IndexedArray(_)
                        | ShellValue::AssociativeArray(_)
                        // TODO(dynamic): confirm this
                        | ShellValue::Dynamic { .. } => {
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
                            Ok(std::format!("{name}={assignable_value_str}").into())
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
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let came_from_undefined = expanded_parameter.undefined;

                //
                // For typing reasons (issues with FnMut and our mut use of self), we can't use
                // transform_expansion. Instead, we inline its logic here.
                //

                let mut transformed_fields = vec![];
                for field in expanded_parameter.fields {
                    let s = String::from(field);
                    let transformed = self
                        .apply_transform_to(&op, &s, came_from_undefined)
                        .await?;
                    transformed_fields.push(WordField::from(transformed));
                }

                Ok(Expansion {
                    fields: transformed_fields,
                    concatenate: expanded_parameter.concatenate,
                    kind: expanded_parameter.kind,
                    undefined: expanded_parameter.undefined,
                })
            }
            brush_parser::word::ParameterExpr::UppercaseFirstChar {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;

                transform_expansion(expanded_parameter, async |s| {
                    Self::pattern_to_first_char(s, expanded_pattern.as_ref(), |c| c.to_uppercase())
                })
                .await
            }
            brush_parser::word::ParameterExpr::UppercasePattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;

                transform_expansion(expanded_parameter, async |s| {
                    Self::pattern_to_string(s.as_str(), expanded_pattern.as_ref(), |str| {
                        str.to_uppercase()
                    })
                })
                .await
            }
            brush_parser::word::ParameterExpr::LowercaseFirstChar {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;

                transform_expansion(expanded_parameter, async |s| {
                    Self::pattern_to_first_char(s, expanded_pattern.as_ref(), |c| c.to_lowercase())
                })
                .await
            }
            brush_parser::word::ParameterExpr::LowercasePattern {
                parameter,
                indirect,
                pattern,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;
                let expanded_pattern = self
                    .basic_expand_opt_pattern(pattern.as_deref(), context)
                    .await?;

                transform_expansion(expanded_parameter, async |s| {
                    Self::pattern_to_string(s.as_str(), expanded_pattern.as_ref(), |str| {
                        str.to_lowercase()
                    })
                })
                .await
            }
            brush_parser::word::ParameterExpr::ReplaceSubstring {
                parameter,
                indirect,
                pattern,
                replacement,
                match_kind,
            } => {
                let expanded_parameter =
                    self.expand_parameter(&parameter, indirect, context).await?;

                // The pattern is a word of its own, but after an anchor (`${x/#~/y}`) it
                // doesn't start where the word as written does, so a tilde there stands
                // for itself.
                let pattern_context = match match_kind {
                    brush_parser::word::SubstringMatchKind::Prefix
                    | brush_parser::word::SubstringMatchKind::Suffix => {
                        context.own_word().mid_word()
                    }
                    brush_parser::word::SubstringMatchKind::FirstOccurrence
                    | brush_parser::word::SubstringMatchKind::Anywhere => context.own_word(),
                };

                let expanded_pattern = self
                    .basic_expand_pattern(pattern.as_str(), pattern_context)
                    .await?
                    .set_extended_globbing(self.parser_options.enable_extended_globbing)
                    .set_case_insensitive(self.shell.options().case_insensitive_conditionals);

                // If no replacement was provided, then we replace with an empty string.
                let replacement = replacement.unwrap_or(String::new());
                let expanded_replacement =
                    self.expand_own_word_to_str(&replacement, context).await?;

                let regex = expanded_pattern.to_regex(
                    matches!(match_kind, brush_parser::word::SubstringMatchKind::Prefix),
                    matches!(match_kind, brush_parser::word::SubstringMatchKind::Suffix),
                )?;

                transform_expansion(expanded_parameter, async |s| {
                    Ok(Self::replace_substring(
                        s.as_str(),
                        &regex,
                        expanded_replacement.as_str(),
                        &match_kind,
                    ))
                })
                .await
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
                        .env()
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
                        kind: ExpansionKind::ElementList,
                        undefined: false,
                    })
                }
            }
            brush_parser::word::ParameterExpr::MemberKeys {
                variable_name,
                concatenate,
            } => {
                let keys = if let Some((_, var)) = self.shell.env().get(variable_name) {
                    var.value().element_keys(self.shell)
                } else {
                    vec![]
                };

                Ok(Expansion {
                    fields: keys
                        .into_iter()
                        .map(|key| WordField(vec![ExpansionPiece::Splittable(key)]))
                        .collect(),
                    concatenate,
                    kind: ExpansionKind::ElementList,
                    undefined: false,
                })
            }
        }
    }

    async fn assign_to_parameter<T: Into<String>>(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        value: T,
        context: WordContext,
    ) -> Result<(), error::Error> {
        let (variable_name, index) = match parameter {
            brush_parser::word::Parameter::Named(name) => (name, None),
            brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                let is_set_assoc_array = if let Some((_, var)) = self.shell.env().get(name) {
                    matches!(
                        var.value(),
                        ShellValue::AssociativeArray(_)
                            | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
                    )
                } else {
                    false
                };

                let index_to_use = self
                    .expand_array_index(index.as_str(), is_set_assoc_array, context)
                    .await?;
                (name, Some(index_to_use))
            }
            brush_parser::word::Parameter::Positional(_)
            | brush_parser::word::Parameter::NamedWithAllIndices {
                name: _,
                concatenate: _,
            }
            | brush_parser::word::Parameter::Special(_) => {
                return Err(error::ErrorKind::CannotAssignToSpecialParameter.into());
            }
        };

        let value = value.into();

        if let Some(index) = index {
            self.shell.env_mut().update_or_add_array_element(
                variable_name,
                index,
                value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )
        } else {
            self.shell.env_mut().update_or_add(
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
        context: WordContext,
    ) -> Result<(Option<String>, Option<String>, Option<ShellVariable>), error::Error> {
        if !indirect {
            Ok(self.try_resolve_parameter_to_variable_without_indirect(parameter))
        } else {
            let expansion = self.expand_parameter(parameter, false, context).await?;
            let parameter_str: String = self.fields_to_string(expansion);
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
            .and_then(|name| self.shell.env().get(name).map(|(_, var)| var.clone()));

        (name, index, var)
    }

    fn undefined_expansion(
        &self,
        parameter: &brush_parser::word::Parameter,
        allow_unset_vars: bool,
    ) -> Result<Expansion, error::Error> {
        if allow_unset_vars || !self.shell.options().treat_unset_variables_as_error {
            Ok(Expansion::undefined())
        } else {
            let err: error::Error =
                error::ErrorKind::ExpandingUnsetVariable(parameter.to_string()).into();
            Err(err.into_fatal())
        }
    }

    async fn expand_parameter(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        indirect: bool,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        self.expand_parameter_internal(parameter, indirect, false, context)
            .await
    }

    async fn expand_parameter_allowing_unset(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        indirect: bool,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        self.expand_parameter_internal(parameter, indirect, true, context)
            .await
    }

    async fn expand_parameter_internal(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        indirect: bool,
        allow_unset_vars: bool,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        let expansion = self
            .expand_parameter_without_indirect(parameter, allow_unset_vars, context)
            .await?;
        if !indirect {
            Ok(expansion)
        } else {
            let parameter_str: String = self.fields_to_string(expansion);
            let inner_parameter =
                brush_parser::word::parse_parameter(parameter_str.as_str(), &self.parser_options)?;

            self.expand_parameter_without_indirect(&inner_parameter, allow_unset_vars, context)
                .await
        }
    }

    async fn expand_parameter_without_indirect(
        &mut self,
        parameter: &brush_parser::word::Parameter,
        allow_unset_vars: bool,
        context: WordContext,
    ) -> Result<Expansion, error::Error> {
        match parameter {
            brush_parser::word::Parameter::Positional(p) => {
                if *p == 0 {
                    Ok(self
                        .expand_special_parameter(&brush_parser::word::SpecialParameter::ShellName))
                } else if let Some(parameter) =
                    self.shell.current_shell_args().get((p - 1) as usize)
                {
                    Ok(Expansion::from(parameter.to_owned()))
                } else {
                    self.undefined_expansion(parameter, allow_unset_vars)
                }
            }
            brush_parser::word::Parameter::Special(s) => Ok(self.expand_special_parameter(s)),
            brush_parser::word::Parameter::Named(n) => {
                if !env::valid_variable_name(n.as_str()) {
                    Err(error::ErrorKind::BadSubstitution(n.clone()).into())
                } else if let Some((_, var)) = self.shell.env().get(n) {
                    if matches!(var.value(), ShellValue::Unset(_)) {
                        self.undefined_expansion(parameter, allow_unset_vars)
                    } else {
                        let value = var.value().try_get_cow_str(self.shell);
                        if let Some(value) = value {
                            Ok(Expansion::from(value.to_string()))
                        } else {
                            self.undefined_expansion(parameter, allow_unset_vars)
                        }
                    }
                } else {
                    self.undefined_expansion(parameter, allow_unset_vars)
                }
            }
            brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                // First check to see if it's an associative array.
                let is_set_assoc_array = if let Some((_, var)) = self.shell.env().get(name) {
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
                    .expand_array_index(index.as_str(), is_set_assoc_array, context)
                    .await?;

                // Index into the array.
                if let Some((_, var)) = self.shell.env().get(name)
                    && let Ok(Some(value)) = var.value().get_at(index_to_use.as_str(), self.shell)
                {
                    Ok(Expansion::from(value.to_string()))
                } else {
                    self.undefined_expansion(parameter, allow_unset_vars)
                }
            }
            brush_parser::word::Parameter::NamedWithAllIndices { name, concatenate } => {
                if let Some((_, var)) = self.shell.env().get(name) {
                    // Resolve a dynamic value once, so the kind and the element values
                    // can't disagree: a getter like RANDOM's changes on every read.
                    let resolved;
                    let value = match var.value() {
                        ShellValue::Dynamic { getter, .. } => {
                            resolved = getter(self.shell);
                            &resolved
                        }
                        other => other,
                    };
                    let fields =
                        list_element_fields(value.element_values(self.shell), value.is_array());

                    Ok(Expansion {
                        fields,
                        concatenate: *concatenate,
                        kind: match value {
                            // Slicing addresses an indexed array by subscript, so it needs
                            // the subscripts themselves and not just the element count.
                            ShellValue::IndexedArray(array) => {
                                ExpansionKind::IndexedArray(array.keys().copied().collect())
                            }
                            // A scalar written `${x[@]}`: bash slices it as a string.
                            ShellValue::String(_) => ExpansionKind::ScalarAsArray,
                            // Associative arrays land here. bash documents substring
                            // expansion of one as producing undefined results -- it
                            // selects from an offset of 1 rather than 0 -- so we
                            // deliberately address them like any other element list.
                            _ => ExpansionKind::ElementList,
                        },
                        undefined: false,
                    })
                } else {
                    Ok(Expansion {
                        fields: vec![],
                        concatenate: *concatenate,
                        kind: ExpansionKind::ElementList,
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
        context: WordContext,
    ) -> Result<String, error::Error> {
        let index_to_use = if for_set_associative_array {
            self.expand_own_word_to_str(index, context).await?
        } else {
            arithmetic::expand_and_eval(self.shell, self.params, index, false)
                .await?
                .to_string()
        };

        Ok(index_to_use)
    }

    fn expand_special_parameter(
        &self,
        parameter: &brush_parser::word::SpecialParameter,
    ) -> Expansion {
        match parameter {
            brush_parser::word::SpecialParameter::AllPositionalParameters { concatenate } => {
                Expansion {
                    fields: list_element_fields(self.shell.current_shell_args().to_vec(), true),
                    concatenate: *concatenate,
                    kind: ExpansionKind::ElementList,
                    undefined: false,
                }
            }
            brush_parser::word::SpecialParameter::PositionalParameterCount => {
                Expansion::from(self.shell.current_shell_args().len().to_string())
            }
            brush_parser::word::SpecialParameter::LastExitStatus => {
                Expansion::from(self.shell.last_exit_status().to_string())
            }
            brush_parser::word::SpecialParameter::CurrentOptionFlags => {
                Expansion::from(self.shell.options().option_flags())
            }
            brush_parser::word::SpecialParameter::ProcessId => {
                Expansion::from(std::process::id().to_string())
            }
            brush_parser::word::SpecialParameter::LastBackgroundProcessId => {
                if let Some(job) = self.shell.jobs().current_job()
                    && let Some(pid) = job.representative_pid()
                {
                    return Expansion::from(pid.to_string());
                }
                Expansion::from(String::new())
            }
            brush_parser::word::SpecialParameter::ShellName => Expansion::from(
                self.shell
                    .current_shell_name()
                    .map_or_else(String::new, |name| name.to_string()),
            ),
        }
    }

    async fn expand_arithmetic_expr(
        &mut self,
        expr: brush_parser::ast::UnexpandedArithmeticExpr,
    ) -> Result<String, error::Error> {
        let value = expr.eval(self.shell, self.params, false).await?;
        Ok(value.to_string())
    }

    fn pattern_to_first_char<F, I>(
        s: String,
        pattern: Option<&patterns::Pattern>,
        transform: F,
    ) -> Result<String, error::Error>
    where
        F: Fn(char) -> I,
        I: Iterator<Item = char>,
    {
        if let Some(first_char) = s.chars().next() {
            let applicable = if let Some(pattern) = pattern {
                pattern.is_empty() || pattern.exactly_matches(first_char.to_string().as_str())?
            } else {
                true
            };

            if applicable {
                if let Some(upper_char) = transform(first_char).next() {
                    let mut result = upper_char.to_string();
                    result.extend(s.chars().skip(1));
                    return Ok(result);
                }
            }
        }

        Ok(s)
    }

    fn pattern_to_string<F>(
        s: &str,
        pattern: Option<&patterns::Pattern>,
        transform: F,
    ) -> Result<String, error::Error>
    where
        F: Fn(&str) -> String,
    {
        if let Some(pattern) = pattern {
            if !pattern.is_empty() {
                let regex = pattern.to_regex(false, false)?;
                let result = regex
                    .replace_all(s.as_ref(), |caps: &fancy_regex::Captures<'_, str>| {
                        transform(&caps[0])
                    });
                Ok(result.into_owned())
            } else {
                Ok(transform(s))
            }
        } else {
            Ok(transform(s))
        }
    }

    fn replace_substring(
        s: &str,
        regex: &fancy_regex::Regex,
        replacement: &str,
        match_kind: &SubstringMatchKind,
    ) -> String {
        // The replacement is already fully expanded; it must be inserted verbatim. A bare
        // `&str` replacer would instead read `$1`, `$0` and `${name}` as references to the
        // regex's capture groups.
        let replacement = fancy_regex::NoExpand(replacement);

        match match_kind {
            brush_parser::word::SubstringMatchKind::Prefix
            | brush_parser::word::SubstringMatchKind::Suffix
            | brush_parser::word::SubstringMatchKind::FirstOccurrence => {
                regex.replace(s, replacement).into_owned()
            }

            brush_parser::word::SubstringMatchKind::Anywhere => {
                regex.replace_all(s, replacement).into_owned()
            }
        }
    }

    async fn apply_transform_to(
        &mut self,
        op: &ParameterTransformOp,
        s: &str,
        came_from_undefined: bool,
    ) -> Result<String, error::Error> {
        match op {
            brush_parser::word::ParameterTransformOp::PromptExpand => {
                prompt::expand_prompt(self.shell, self.params, s).await
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
                if came_from_undefined {
                    Ok(String::new())
                } else {
                    // TODO(expansion): This isn't right for arrays.
                    // TODO(expansion): This doesn't honor 'separate_words'
                    Ok(escape::force_quote(s, escape::QuoteMode::SingleQuote))
                }
            }
            brush_parser::word::ParameterTransformOp::Quoted => {
                if came_from_undefined {
                    Ok(String::new())
                } else {
                    Ok(escape::force_quote(s, escape::QuoteMode::SingleQuote))
                }
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

fn list_element_fields(values: Vec<String>, preserve_empty_elements: bool) -> Vec<WordField> {
    let value_count = values.len();
    values
        .into_iter()
        .enumerate()
        .map(|(i, value)| {
            let piece = if preserve_empty_elements {
                ExpansionPiece::list_element(value, i + 1 < value_count)
            } else {
                ExpansionPiece::Splittable(value)
            };
            WordField(vec![piece])
        })
        .collect()
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

            // TODO(expansion): What if expansions have different concatenation values?
            acc.concatenate = expansion.concatenate;
            // The last kind wins, like `concatenate` above. An IndexedArray kind's
            // subscripts no longer describe the merged fields; nothing slices a coalesced
            // expansion today, and `polymorphic_subslice` must not until that's fixed.
            acc.kind = expansion.kind;

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

async fn transform_expansion<F, FReturn>(
    expansion: Expansion,
    mut f: F,
) -> Result<Expansion, error::Error>
where
    F: FnMut(String) -> FReturn,
    FReturn: Future<Output = Result<String, error::Error>>,
{
    let mut transformed_fields = vec![];
    for field in expansion.fields {
        let transformed_field = WordField::from(f(String::from(field)).await?);
        transformed_fields.push(transformed_field);
    }

    Ok(Expansion {
        fields: transformed_fields,
        concatenate: expansion.concatenate,
        kind: expansion.kind,
        undefined: expansion.undefined,
    })
}

fn may_contain_braces_to_expand(s: &str) -> bool {
    // This is a completely inaccurate but quick heuristic used to see if
    // it's even worth properly parsing the string to find brace expressions.
    // It's mostly used to avoid more expensive parsing just because we've
    // encountered a brace used in a parameter expansion.
    let mut last_was_unescaped_dollar_sign = false;
    let mut last_was_escape = false;
    let mut saw_opening_brace = false;
    let mut saw_closing_brace = false;
    for c in s.chars() {
        if !last_was_unescaped_dollar_sign {
            if c == '{' {
                saw_opening_brace = true;
            } else if c == '}' {
                saw_closing_brace = true;
                if saw_opening_brace {
                    return true;
                }
            }
        }

        last_was_unescaped_dollar_sign = !last_was_escape && c == '$';
        last_was_escape = c == '\\';
    }

    saw_opening_brace && saw_closing_brace
}

#[expect(clippy::panic_in_result_fn)]
#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[tokio::test]
    async fn test_full_expansion() -> Result<()> {
        let mut shell = crate::shell::Shell::builder().build().await?;
        let params = shell.default_exec_params();

        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "\"\"").await?,
            vec![""]
        );
        // Literal text is never field-split (the parser would not produce such a word).
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "a b").await?,
            vec!["a b"]
        );
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "ab").await?,
            vec!["ab"]
        );
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, r#""a b""#).await?,
            vec!["a b"]
        );
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "").await?,
            Vec::<String>::new()
        );
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "$@").await?,
            Vec::<String>::new()
        );
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "$*").await?,
            Vec::<String>::new()
        );

        Ok(())
    }

    /// Regression test: a quoted empty string `""` must survive word expansion
    /// even when `nullglob` is enabled.  Previously, `Pattern::expand()` would
    /// return `NoGlob` for an all-empty pattern, which with nullglob set caused
    /// `expand_pathnames_in_field` to silently discard the empty-string field.
    #[tokio::test]
    async fn test_quoted_empty_string_with_nullglob() -> Result<()> {
        let mut shell = crate::shell::Shell::builder().build().await?;
        shell.options_mut().expand_non_matching_patterns_to_null = true; // shopt -s nullglob
        let params = shell.default_exec_params();

        // Quoted empty string must always produce exactly one empty-string field.
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "\"\"").await?,
            vec![""]
        );
        // Unquoted empty string (no characters at all) should still produce no fields.
        assert_eq!(
            full_expand_and_split_word(&mut shell, &params, "").await?,
            Vec::<String>::new()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_brace_expansion() -> Result<()> {
        let mut shell = crate::shell::Shell::builder().build().await?;
        let params = shell.default_exec_params();
        let expander = WordExpander::new(&mut shell, &params, WordKind::Command);

        let expand = |w| expander.brace_expand_if_needed(w);
        assert_eq!(expand("abc"), None);
        assert_eq!(expand("a{,b}d"), Some(vec!["ad".into(), "abd".into()]));
        assert_eq!(expand("a{b,c}d"), Some(vec!["abd".into(), "acd".into()]));
        assert_eq!(
            expand("a{1..3}d"),
            Some(vec!["a1d".into(), "a2d".into(), "a3d".into()])
        );
        assert_eq!(expand(r#""{a,b}""#), Some(vec![r#""{a,b}""#.into()]));
        assert_eq!(expand("a{}b"), Some(vec!["a{}b".into()]));
        assert_eq!(expand("a{ }b"), Some(vec!["a{ }b".into()]));
        assert_eq!(
            expand("{a,b{1,2}}"),
            Some(vec!["a".into(), "b1".into(), "b2".into()])
        );

        Ok(())
    }

    #[test]
    fn test_replace_substring_inserts_replacement_verbatim() -> Result<()> {
        // A capture group, as an extglob pattern like `@(b)` produces.
        let regex = fancy_regex::Regex::new("(b)")?;
        let replace = |r: &str, kind: SubstringMatchKind| {
            WordExpander::<crate::extensions::DefaultShellExtensions>::replace_substring(
                "abcb", &regex, r, &kind,
            )
        };

        assert_eq!(replace("$1", SubstringMatchKind::FirstOccurrence), "a$1cb");
        assert_eq!(replace("$0", SubstringMatchKind::Anywhere), "a$0c$0");
        assert_eq!(
            replace("${y}$$", SubstringMatchKind::Anywhere),
            "a${y}$$c${y}$$"
        );

        Ok(())
    }

    #[test]
    fn test_to_initial_capitals() {
        assert_eq!(to_initial_capitals("ab bc cd"), String::from("Ab Bc Cd"));
        assert_eq!(to_initial_capitals(" a "), String::from(" A "));
        assert_eq!(to_initial_capitals(""), String::new());
    }
}
