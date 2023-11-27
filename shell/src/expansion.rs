use anyhow::Result;

use crate::shell::Shell;
pub struct WordExpander<'a> {
    shell: &'a Shell,
}

impl<'a> WordExpander<'a> {
    pub fn new(shell: &'a Shell) -> Self {
        Self { shell }
    }

    pub fn expand(&self, word: &str) -> Result<String> {
        let pieces = parser::parse_word_for_expansion(word)?;
        let expanded_pieces = pieces
            .iter()
            .map(|p| p.expand(self.shell))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        let expansion = expanded_pieces.concat();
        Ok(expansion)
    }
}

pub trait Expandable {
    fn expand(&self, shell: &Shell) -> Result<String>;
}

impl Expandable for parser::word::WordPiece {
    fn expand(&self, shell: &Shell) -> Result<String> {
        let expansion = match self {
            // TODO: Handle escape sequences inside text or double-quoted text! Probably need to parse them differently.
            parser::word::WordPiece::Text(t) => t.to_owned(),
            parser::word::WordPiece::SingleQuotedText(t) => t.to_owned(),
            parser::word::WordPiece::DoubleQuotedSequence(pieces) => {
                let expanded_pieces = pieces
                    .iter()
                    .map(|p| p.expand(shell))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?;
                expanded_pieces.concat()
            }
            parser::word::WordPiece::TildePrefix(prefix) => expand_tilde_expression(shell, prefix)?,
            parser::word::WordPiece::ParameterExpansion(p) => p.expand(shell)?,
        };

        Ok(expansion)
    }
}

fn expand_tilde_expression(shell: &Shell, prefix: &str) -> Result<String> {
    if prefix != "" {
        log::error!("UNIMPLEMENTED: complex tilde expression: {}", prefix);
        todo!("expansion: complex tilde expression");
    }

    if let Some(home) = shell.variables.get("HOME") {
        Ok(home.value.to_owned())
    } else {
        Err(anyhow::anyhow!(
            "cannot expand tilde expression with HOME not set"
        ))
    }
}

impl Expandable for parser::word::ParameterExpression {
    fn expand(&self, shell: &Shell) -> Result<String> {
        match self {
            parser::word::ParameterExpression::Parameter { parameter } => parameter.expand(shell),
            parser::word::ParameterExpression::UseDefaultValues {
                parameter: _,
                test_type: _,
                default_value: _,
            } => todo!("expansion: use default values expressions"),
            parser::word::ParameterExpression::AssignDefaultValues {
                parameter: _,
                test_type: _,
                default_value: _,
            } => todo!("expansion: assign default values expressions"),
            parser::word::ParameterExpression::IndicateErrorIfNullOrUnset {
                parameter: _,
                test_type: _,
                error_message: _,
            } => todo!("expansion: indicate error if null or unset expressions"),
            parser::word::ParameterExpression::UseAlternativeValue {
                parameter: _,
                test_type: _,
                alternative_value: _,
            } => todo!("expansion: use alternative value expressions"),
            parser::word::ParameterExpression::StringLength { parameter: _ } => {
                todo!("expansion: string length expression")
            }
            parser::word::ParameterExpression::RemoveSmallestSuffixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove smallest suffix pattern expressions"),
            parser::word::ParameterExpression::RemoveLargestSuffixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove largest suffix pattern expressions"),
            parser::word::ParameterExpression::RemoveSmallestPrefixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove smallest prefix pattern expressions"),
            parser::word::ParameterExpression::RemoveLargestPrefixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove largest prefix pattern expressions"),
        }
    }
}

impl Expandable for parser::word::Parameter {
    fn expand(&self, shell: &Shell) -> Result<String> {
        match self {
            parser::word::Parameter::Positional(_p) => todo!("positional parameter expansion"),
            parser::word::Parameter::Special(s) => s.expand(shell),
            parser::word::Parameter::Named(n) => Ok(shell
                .variables
                .get(n)
                .map_or_else(|| "".to_owned(), |v| v.value.to_owned())),
        }
    }
}

impl Expandable for parser::word::SpecialParameter {
    fn expand(&self, shell: &Shell) -> Result<String> {
        match self {
            parser::word::SpecialParameter::AllPositionalParameters { concatenate: _ } => {
                todo!("expansion: all positional parameters")
            }
            parser::word::SpecialParameter::PositionalParameterCount => {
                Ok(shell.positional_parameters.len().to_string())
            }
            parser::word::SpecialParameter::LastExitStatus => {
                Ok(shell.last_exit_status.to_string())
            }
            parser::word::SpecialParameter::CurrentOptionFlags => Ok(shell.current_option_flags()),
            parser::word::SpecialParameter::ProcessId => Ok(std::process::id().to_string()),
            parser::word::SpecialParameter::LastBackgroundProcessId => {
                todo!("expansion: last background process id")
            }
            parser::word::SpecialParameter::ShellName => Ok(shell
                .shell_name
                .as_ref()
                .map_or_else(|| "".to_owned(), |name| name.to_owned())),
        }
    }
}
