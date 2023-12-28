use anyhow::Result;
use parser::ast;

use crate::arithmetic::Evaluatable;
use crate::shell::Shell;
use crate::variables::ShellVariable;

pub(crate) fn expand_word(shell: &mut Shell, word: &ast::Word) -> Result<String> {
    let mut expander = WordExpander::new(shell);
    expander.expand(word.flatten().as_str())
}

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

        Ok(expanded_pieces.concat())
    }
}

pub trait Expandable {
    fn expand(&self, shell: &mut Shell) -> Result<String>;
}

impl Expandable for parser::word::WordPiece {
    fn expand(&self, shell: &mut Shell) -> Result<String> {
        let expansion = match self {
            parser::word::WordPiece::Text(t) => t.clone(),
            parser::word::WordPiece::SingleQuotedText(t) => t.clone(),
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
                let exec_output = exec_result.output;

                if exec_output.is_none() {
                    log::error!("No output captured");
                }

                let exec_output = exec_output.unwrap_or_else(String::new);

                // We trim trailing newlines, per spec.
                let exec_output = exec_output.trim_end_matches('\n');

                exec_output.to_owned()
            }
            parser::word::WordPiece::EscapeSequence(s) => s.strip_prefix('\\').unwrap().to_owned(),
            parser::word::WordPiece::ArithmeticExpression(e) => e.expand(shell)?,
        };

        Ok(expansion)
    }
}

fn expand_tilde_expression(shell: &Shell, prefix: &str) -> Result<String> {
    if !prefix.is_empty() {
        log::error!("UNIMPLEMENTED: complex tilde expression: {}", prefix);
        todo!("expansion: complex tilde expression");
    }

    if let Some(home) = shell.env.get("HOME") {
        Ok(String::from(&home.value))
    } else {
        Err(anyhow::anyhow!(
            "cannot expand tilde expression with HOME not set"
        ))
    }
}

impl Expandable for parser::word::ParameterExpr {
    fn expand(&self, shell: &mut Shell) -> Result<String> {
        // TODO: observe test_type
        #[allow(clippy::cast_possible_truncation)]
        match self {
            parser::word::ParameterExpr::Parameter { parameter } => parameter.expand(shell),
            parser::word::ParameterExpr::UseDefaultValues {
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
                    Ok(String::new())
                }
            }
            parser::word::ParameterExpr::AssignDefaultValues {
                parameter: _,
                test_type: _,
                default_value: _,
            } => todo!("expansion: assign default values expressions"),
            parser::word::ParameterExpr::IndicateErrorIfNullOrUnset {
                parameter: _,
                test_type: _,
                error_message: _,
            } => todo!("expansion: indicate error if null or unset expressions"),
            parser::word::ParameterExpr::UseAlternativeValue {
                parameter,
                test_type: _,
                alternative_value,
            } => {
                let expanded_parameter = parameter.expand(shell)?;
                if !expanded_parameter.is_empty() {
                    Ok(WordExpander::new(shell)
                        .expand(alternative_value.as_ref().map_or("", |av| av.as_str()))?)
                } else {
                    Ok(String::new())
                }
            }
            parser::word::ParameterExpr::StringLength { parameter } => {
                let expanded_parameter = parameter.expand(shell)?;
                Ok(expanded_parameter.len().to_string())
            }
            parser::word::ParameterExpr::RemoveSmallestSuffixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove smallest suffix pattern expressions"),
            parser::word::ParameterExpr::RemoveLargestSuffixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove largest suffix pattern expressions"),
            parser::word::ParameterExpr::RemoveSmallestPrefixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove smallest prefix pattern expressions"),
            parser::word::ParameterExpr::RemoveLargestPrefixPattern {
                parameter: _,
                pattern: _,
            } => todo!("expansion: remove largest prefix pattern expressions"),
            parser::word::ParameterExpr::Substring {
                parameter,
                offset,
                length,
            } => {
                let expanded_parameter = parameter.expand(shell)?;

                // TODO: handle negative offset
                let expanded_offset = offset.eval(shell)?;
                let expanded_offset = usize::try_from(expanded_offset)?;

                if expanded_offset >= expanded_parameter.len() {
                    return Ok(String::new());
                }

                let result = if let Some(length) = length {
                    let expanded_length = length.eval(shell)?;
                    if expanded_length < 0 {
                        log::error!("UNIMPLEMENTED: substring with negative length");
                        todo!("substring with negative length");
                    }

                    let expanded_length = std::cmp::min(
                        usize::try_from(expanded_length)?,
                        expanded_parameter.len() - expanded_offset,
                    );

                    &expanded_parameter[expanded_offset..(expanded_offset + expanded_length)]
                } else {
                    &expanded_parameter[expanded_offset..]
                };

                Ok(result.to_owned())
            }
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
                .env
                .get(n)
                .map_or_else(String::new, |v| String::from(&v.value))),
            parser::word::Parameter::NamedWithIndex { name, index } => match shell.env.get(name) {
                Some(ShellVariable { value, .. }) => Ok(value
                    .get_at(*index)
                    .map_or_else(String::new, |s| s.to_owned())),
                None => Ok(String::new()),
            },
            parser::word::Parameter::NamedWithAllIndices { name, concatenate } => {
                match shell.env.get(name) {
                    Some(ShellVariable { value, .. }) => Ok(value.get_all(*concatenate)),
                    None => Ok(String::new()),
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
                .map_or_else(String::new, |name| name.clone())),
        }
    }
}

impl Expandable for parser::ast::ArithmeticExpr {
    fn expand(&self, shell: &mut Shell) -> Result<String> {
        let value = self.eval(shell)?;
        Ok(value.to_string())
    }
}
