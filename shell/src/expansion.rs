use anyhow::Result;

use crate::context::ExecutionContext;

pub struct WordExpander<'a> {
    context: &'a ExecutionContext,
}

impl<'a> WordExpander<'a> {
    pub fn new(context: &'a ExecutionContext) -> Self {
        Self { context }
    }

    pub fn expand(&self, word: &str) -> Result<String> {
        let pieces = parser::parse_word_for_expansion(word)?;
        let expanded_pieces = pieces
            .iter()
            .map(|p| p.expand(self.context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        let expansion = expanded_pieces.concat();

        // DBG:RRO
        log::debug!("expand: {{{}}} -> {{{}}}", word, expansion);

        Ok(expansion)
    }
}

pub trait Expandable {
    fn expand(&self, context: &ExecutionContext) -> Result<String>;
}

impl Expandable for parser::word::WordPiece {
    fn expand(&self, context: &ExecutionContext) -> Result<String> {
        let expansion = match self {
            // TODO: Handle escape sequences inside text or double-quoted text! Probably need to parse them differently.
            parser::word::WordPiece::Text(t) => t.to_owned(),
            parser::word::WordPiece::SingleQuotedText(t) => t.to_owned(),
            parser::word::WordPiece::DoubleQuotedSequence(pieces) => {
                let expanded_pieces = pieces
                    .iter()
                    .map(|p| p.expand(context))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?;
                expanded_pieces.concat()
            }
            parser::word::WordPiece::TildePrefix(_t) => todo!("tilde prefix expansion"),
            parser::word::WordPiece::ParameterExpansion(p) => p.expand(context)?,
        };

        Ok(expansion)
    }
}

impl Expandable for parser::word::ParameterExpression {
    fn expand(&self, context: &ExecutionContext) -> Result<String> {
        match self {
            parser::word::ParameterExpression::Parameter { parameter } => parameter.expand(context),
            parser::word::ParameterExpression::UseDefaultValues {
                parameter,
                test_type,
                default_value,
            } => todo!("expansion: use default values expressions"),
            parser::word::ParameterExpression::AssignDefaultValues {
                parameter,
                test_type,
                default_value,
            } => todo!("expansion: assign default values expressions"),
            parser::word::ParameterExpression::IndicateErrorIfNullOrUnset {
                parameter,
                test_type,
                error_message,
            } => todo!("expansion: indicate error if null or unset expressions"),
            parser::word::ParameterExpression::UseAlternativeValue {
                parameter,
                test_type,
                alternative_value,
            } => todo!("expansion: use alternative value expressions"),
            parser::word::ParameterExpression::StringLength { parameter } => {
                todo!("expansion: string length expression")
            }
            parser::word::ParameterExpression::RemoveSmallestSuffixPattern {
                parameter,
                pattern,
            } => todo!("expansion: remove smallest suffix pattern expressions"),
            parser::word::ParameterExpression::RemoveLargestSuffixPattern {
                parameter,
                pattern,
            } => todo!("expansion: remove largest suffix pattern expressions"),
            parser::word::ParameterExpression::RemoveSmallestPrefixPattern {
                parameter,
                pattern,
            } => todo!("expansion: remove smallest prefix pattern expressions"),
            parser::word::ParameterExpression::RemoveLargestPrefixPattern {
                parameter,
                pattern,
            } => todo!("expansion: remove largest prefix pattern expressions"),
        }
    }
}

impl Expandable for parser::word::Parameter {
    fn expand(&self, context: &ExecutionContext) -> Result<String> {
        match self {
            parser::word::Parameter::Positional(p) => todo!("positional parameter expansion"),
            parser::word::Parameter::Special(s) => s.expand(context),
            parser::word::Parameter::Named(n) => Ok(context
                .parameters
                .get(n)
                .map_or_else(|| "".to_owned(), |v| v.to_owned())),
        }
    }
}

impl Expandable for parser::word::SpecialParameter {
    fn expand(&self, context: &ExecutionContext) -> Result<String> {
        match self {
            parser::word::SpecialParameter::AllPositionalParameters { concatenate } => {
                todo!("expansion: all positional parameters")
            }
            parser::word::SpecialParameter::PositionalParameterCount => {
                todo!("expansion: positional parameter count")
            }
            parser::word::SpecialParameter::LastExitStatus => {
                Ok(context.last_pipeline_exit_status.to_string())
            }
            parser::word::SpecialParameter::CurrentOptionFlags => {
                todo!("expansion: current option flags")
            }
            parser::word::SpecialParameter::ProcessId => todo!("expansion: process id"),
            parser::word::SpecialParameter::LastBackgroundProcessId => {
                todo!("expansion: last background process id")
            }
            parser::word::SpecialParameter::ShellName => todo!("expansion: shell name"),
        }
    }
}
