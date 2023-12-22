use anyhow::Result;

use crate::shell::Shell;
use crate::variables::ShellVariable;

pub struct WordExpander<'a> {
    shell: &'a mut Shell,
}

impl<'a> WordExpander<'a> {
    pub fn new(shell: &'a mut Shell) -> Self {
        Self { shell }
    }

    pub fn expand(&mut self, word: &str) -> Result<String> {
        // Expand: tildes, parameters, command substitutions, arithmetic.
        let pieces = parser::parse_word_for_expansion(word)?;
        let expanded_pieces = pieces
            .iter()
            .map(|p| p.expand(self.shell))
            .collect::<Result<Vec<_>>>()?;

        // TODO: Split fields
        // TODO: Expand pathnames
        // TODO: Remove quotes

        let expansion = expanded_pieces.concat();
        Ok(expansion)
    }
}

pub trait Expandable {
    fn expand(&self, shell: &mut Shell) -> Result<String>;
}

impl Expandable for parser::word::WordPiece {
    fn expand(&self, shell: &mut Shell) -> Result<String> {
        let expansion = match self {
            parser::word::WordPiece::Text(t) => t.to_owned(),
            parser::word::WordPiece::SingleQuotedText(t) => t.to_owned(),
            parser::word::WordPiece::DoubleQuotedSequence(pieces) => {
                let expanded_pieces = pieces
                    .iter()
                    .map(|p| p.expand(shell))
                    .collect::<Result<Vec<_>>>()?;
                expanded_pieces.concat()
            }
            parser::word::WordPiece::TildePrefix(prefix) => expand_tilde_expression(shell, prefix)?,
            parser::word::WordPiece::ParameterExpansion(p) => p.expand(shell)?,
            parser::word::WordPiece::CommandSubstitution(s) => {
                let exec_result = shell.run_string(s.as_str(), true)?;
                let exec_output = exec_result
                    .output
                    .ok_or_else(|| anyhow::anyhow!("No output captured"))?;

                // We trim trailing newlines, per spec.
                let exec_output = exec_output.trim_end_matches('\n');

                exec_output.to_owned()
            }
            parser::word::WordPiece::EscapeSequence(s) => s.strip_prefix('\\').unwrap().to_owned(),
        };

        Ok(expansion)
    }
}

fn expand_tilde_expression(shell: &Shell, prefix: &str) -> Result<String> {
    if !prefix.is_empty() {
        log::error!("UNIMPLEMENTED: complex tilde expression: {}", prefix);
        todo!("expansion: complex tilde expression");
    }

    if let Some(home) = shell.variables.get("HOME") {
        Ok(String::from(&home.value))
    } else {
        Err(anyhow::anyhow!(
            "cannot expand tilde expression with HOME not set"
        ))
    }
}

impl Expandable for parser::word::ParameterExpression {
    fn expand(&self, shell: &mut Shell) -> Result<String> {
        // TODO: observe test_type
        match self {
            parser::word::ParameterExpression::Parameter { parameter } => parameter.expand(shell),
            parser::word::ParameterExpression::UseDefaultValues {
                parameter,
                test_type: _,
                default_value,
            } => {
                let expanded_parameter = parameter.expand(shell)?;
                if !expanded_parameter.is_empty() {
                    Ok(expanded_parameter)
                } else if let Some(default_value) = default_value {
                    Ok(WordExpander::new(shell).expand(default_value.as_str())?)
                } else {
                    Ok("".to_owned())
                }
            }
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
                parameter,
                test_type: _,
                alternative_value,
            } => {
                let expanded_parameter = parameter.expand(shell)?;
                if !expanded_parameter.is_empty() {
                    Ok(WordExpander::new(shell)
                        .expand(alternative_value.as_ref().map_or("", |av| av.as_str()))?)
                } else {
                    Ok("".to_owned())
                }
            }
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
    fn expand(&self, shell: &mut Shell) -> Result<String> {
        match self {
            parser::word::Parameter::Positional(p) => {
                if *p == 0 {
                    return Err(anyhow::anyhow!("unexpected positional parameter"));
                }

                let parameter: &str =
                    if let Some(parameter) = shell.positional_parameters.get((p - 1) as usize) {
                        parameter
                    } else {
                        ""
                    };

                Ok(parameter.to_owned())
            }
            parser::word::Parameter::Special(s) => s.expand(shell),
            parser::word::Parameter::Named(n) => Ok(shell
                .variables
                .get(n)
                .map_or_else(|| "".to_owned(), |v| String::from(&v.value))),
            parser::word::Parameter::NamedWithIndex { name, index } => {
                match shell.variables.get(name) {
                    Some(ShellVariable { value, .. }) => Ok(value
                        .get_at(*index)
                        .map_or_else(|| "".to_owned(), |s| s.to_owned())),
                    None => Ok("".to_owned()),
                }
            }
            parser::word::Parameter::NamedWithAllIndices { name, concatenate } => {
                match shell.variables.get(name) {
                    Some(ShellVariable { value, .. }) => Ok(value.get_all(*concatenate)),
                    None => Ok("".to_owned()),
                }
            }
        }
    }
}

impl Expandable for parser::word::SpecialParameter {
    fn expand(&self, shell: &mut Shell) -> Result<String> {
        match self {
            parser::word::SpecialParameter::AllPositionalParameters { concatenate: _ } => {
                // TODO: implement concatenate policy
                Ok(shell.positional_parameters.join(" "))
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
