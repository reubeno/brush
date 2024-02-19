use anyhow::Result;
use parser::ast;

use crate::arithmetic::Evaluatable;
use crate::error;
use crate::patterns;
use crate::prompt;
use crate::shell::Shell;
use crate::variables::ShellVariable;

pub(crate) async fn expand_word(
    shell: &mut Shell,
    word: &ast::Word,
) -> Result<String, error::Error> {
    let mut expander = WordExpander::new(shell);
    expander.expand(word.flatten().as_str()).await
}

async fn expand_pattern(
    shell: &mut Shell,
    pattern: &Option<String>,
) -> Result<String, error::Error> {
    if let Some(pattern) = pattern {
        let mut expander = WordExpander::new(shell);
        expander.expand(pattern.as_str()).await
    } else {
        Ok(String::new())
    }
}

pub struct WordExpander<'a> {
    shell: &'a mut Shell,
}

impl<'a> WordExpander<'a> {
    pub fn new(shell: &'a mut Shell) -> Self {
        Self { shell }
    }

    pub async fn expand(&mut self, word: &str) -> Result<String, error::Error> {
        // Expand: tildes, parameters, command substitutions, arithmetic.
        let pieces = parser::parse_word_for_expansion(word).map_err(error::Error::Unknown)?;

        let mut expanded_pieces = String::new();
        for piece in pieces {
            expanded_pieces.push_str(piece.expand(self.shell).await?.as_str());
        }

        // // TODO: Split fields; observe IFS
        // let fields = expanded_pieces.split(' ');

        // // TODO: Expand pathnames
        // // TODO: skip this if set -f is in effect
        // let expanded_fields = fields
        //     .map(|field| self.expand_pathnames(field))
        //     .into_iter()
        //     .collect::<Result<Vec<_>, error::Error>>()?;

        // let flattened: Vec<String> = expanded_fields.into_iter().flatten().collect();

        // // TODO: Remove quotes here (not above)

        // // TODO: Fix re-joining.
        // let result = flattened.join(" ");

        // Ok(result)

        Ok(expanded_pieces)
    }

    #[allow(dead_code)]
    fn expand_pathnames(&self, s: &str) -> Result<Vec<String>, error::Error> {
        // TODO: handle [] patterns
        let needs_expansion = s.chars().any(|c| c == '*' || c == '?');

        if needs_expansion {
            patterns::pattern_expand(s, &self.shell.working_dir)
        } else {
            Ok(vec![s.to_owned()])
        }
    }
}

#[async_trait::async_trait]
pub trait Expandable {
    async fn expand(&self, shell: &mut Shell) -> Result<String, error::Error>;
}

#[async_trait::async_trait]
impl Expandable for parser::word::WordPiece {
    async fn expand(&self, shell: &mut Shell) -> Result<String, error::Error> {
        let expansion = match self {
            parser::word::WordPiece::Text(t) => t.clone(),
            parser::word::WordPiece::SingleQuotedText(t) => t.clone(),
            parser::word::WordPiece::DoubleQuotedSequence(pieces) => {
                let mut expanded_pieces = String::new();
                for piece in pieces {
                    expanded_pieces.push_str(piece.expand(shell).await?.as_str());
                }
                expanded_pieces
            }
            parser::word::WordPiece::TildePrefix(prefix) => expand_tilde_expression(shell, prefix)?,
            parser::word::WordPiece::ParameterExpansion(p) => p.expand(shell).await?,
            parser::word::WordPiece::CommandSubstitution(s) => {
                let exec_result = shell.run_string(s.as_str(), true).await?;
                let exec_output = exec_result.output;

                if exec_output.is_none() {
                    log::error!("error: no output captured from command substitution");
                }

                let exec_output = exec_output.unwrap_or_else(String::new);

                // We trim trailing newlines, per spec.
                let exec_output = exec_output.trim_end_matches('\n');

                exec_output.to_owned()
            }
            parser::word::WordPiece::EscapeSequence(s) => s.strip_prefix('\\').unwrap().to_owned(),
            parser::word::WordPiece::ArithmeticExpression(e) => e.expand(shell).await?,
        };

        Ok(expansion)
    }
}

fn expand_tilde_expression(shell: &Shell, prefix: &str) -> Result<String, error::Error> {
    if !prefix.is_empty() {
        return error::unimp("expansion: complex tilde expression");
    }

    if let Some(home) = shell.env.get("HOME") {
        Ok(String::from(&home.value))
    } else {
        Err(error::Error::TildeWithoutValidHome)
    }
}

#[async_trait::async_trait]
impl Expandable for parser::word::ParameterExpr {
    #[allow(clippy::too_many_lines)]
    async fn expand(&self, shell: &mut Shell) -> Result<String, error::Error> {
        // TODO: observe test_type
        #[allow(clippy::cast_possible_truncation)]
        match self {
            parser::word::ParameterExpr::Parameter { parameter } => parameter.expand(shell).await,
            parser::word::ParameterExpr::UseDefaultValues {
                parameter,
                test_type: _,
                default_value,
            } => {
                let expanded_parameter = parameter.expand(shell).await?;
                if !expanded_parameter.is_empty() {
                    Ok(expanded_parameter)
                } else if let Some(default_value) = default_value {
                    Ok(WordExpander::new(shell)
                        .expand(default_value.as_str())
                        .await?)
                } else {
                    Ok(String::new())
                }
            }
            parser::word::ParameterExpr::AssignDefaultValues {
                parameter: _,
                test_type: _,
                default_value: _,
            } => error::unimp("expansion: assign default values expressions"),
            parser::word::ParameterExpr::IndicateErrorIfNullOrUnset {
                parameter: _,
                test_type: _,
                error_message: _,
            } => error::unimp("expansion: indicate error if null or unset expressions"),
            parser::word::ParameterExpr::UseAlternativeValue {
                parameter,
                test_type: _,
                alternative_value,
            } => {
                let expanded_parameter = parameter.expand(shell).await?;
                if !expanded_parameter.is_empty() {
                    Ok(WordExpander::new(shell)
                        .expand(alternative_value.as_ref().map_or("", |av| av.as_str()))
                        .await?)
                } else {
                    Ok(String::new())
                }
            }
            parser::word::ParameterExpr::ParameterLength { parameter } => {
                let expanded_parameter = parameter.expand(shell).await?;
                Ok(expanded_parameter.len().to_string())
            }
            parser::word::ParameterExpr::RemoveSmallestSuffixPattern { parameter, pattern } => {
                let expanded_parameter = parameter.expand(shell).await?;
                let expanded_pattern = expand_pattern(shell, pattern).await?;
                let result = patterns::remove_smallest_matching_suffix(
                    expanded_parameter.as_str(),
                    expanded_pattern.as_str(),
                )?;
                Ok(result.to_owned())
            }
            parser::word::ParameterExpr::RemoveLargestSuffixPattern { parameter, pattern } => {
                let expanded_parameter = parameter.expand(shell).await?;
                let expanded_pattern = expand_pattern(shell, pattern).await?;
                let result = patterns::remove_largest_matching_suffix(
                    expanded_parameter.as_str(),
                    expanded_pattern.as_str(),
                )?;

                Ok(result.to_owned())
            }
            parser::word::ParameterExpr::RemoveSmallestPrefixPattern { parameter, pattern } => {
                let expanded_parameter = parameter.expand(shell).await?;
                let expanded_pattern = expand_pattern(shell, pattern).await?;
                let result = patterns::remove_smallest_matching_prefix(
                    expanded_parameter.as_str(),
                    expanded_pattern.as_str(),
                )?;

                Ok(result.to_owned())
            }
            parser::word::ParameterExpr::RemoveLargestPrefixPattern { parameter, pattern } => {
                let expanded_parameter = parameter.expand(shell).await?;
                let expanded_pattern = expand_pattern(shell, pattern).await?;
                let result = patterns::remove_largest_matching_prefix(
                    expanded_parameter.as_str(),
                    expanded_pattern.as_str(),
                )?;

                Ok(result.to_owned())
            }
            parser::word::ParameterExpr::Substring {
                parameter,
                offset,
                length,
            } => {
                let expanded_parameter = parameter.expand(shell).await?;

                // TODO: handle negative offset
                let expanded_offset = offset.eval(shell).await?;
                let expanded_offset = usize::try_from(expanded_offset)
                    .map_err(|e| error::Error::Unknown(e.into()))?;

                if expanded_offset >= expanded_parameter.len() {
                    return Ok(String::new());
                }

                let result = if let Some(length) = length {
                    let expanded_length = length.eval(shell).await?;
                    if expanded_length < 0 {
                        return error::unimp("substring with negative length");
                    }

                    let expanded_length = std::cmp::min(
                        usize::try_from(expanded_length)
                            .map_err(|e| error::Error::Unknown(e.into()))?,
                        expanded_parameter.len() - expanded_offset,
                    );

                    &expanded_parameter[expanded_offset..(expanded_offset + expanded_length)]
                } else {
                    &expanded_parameter[expanded_offset..]
                };

                Ok(result.to_owned())
            }
            parser::word::ParameterExpr::Transform { parameter, op } => {
                let expanded_parameter = parameter.expand(shell).await?;
                match op {
                    parser::word::ParameterTransformOp::PromptExpand => {
                        let result = prompt::expand_prompt(shell, expanded_parameter.as_str())?;
                        Ok(result)
                    }
                    parser::word::ParameterTransformOp::CapitalizeInitial => {
                        error::unimp("parameter transformation: CapitalizeInitial")
                    }
                    parser::word::ParameterTransformOp::ExpandEscapeSequences => {
                        error::unimp("parameter transformation: ExpandEscapeSequences")
                    }
                    parser::word::ParameterTransformOp::PossiblyQuoteWithArraysExpanded {
                        separate_words: _,
                    } => error::unimp("parameter transformation: PossiblyQuoteWithArraysExpanded"),
                    parser::word::ParameterTransformOp::Quoted => {
                        error::unimp("parameter transformation: Quoted")
                    }
                    parser::word::ParameterTransformOp::ToAssignmentLogic => {
                        error::unimp("parameter transformation: ToAssignmentLogic")
                    }
                    parser::word::ParameterTransformOp::ToAttributeFlags => {
                        error::unimp("parameter transformation: ToAttributeFlags")
                    }
                    parser::word::ParameterTransformOp::ToLowerCase => {
                        error::unimp("parameter transformation: ToLowerCase")
                    }
                    parser::word::ParameterTransformOp::ToUpperCase => {
                        error::unimp("parameter transformation: ToUpperCase")
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Expandable for parser::word::Parameter {
    async fn expand(&self, shell: &mut Shell) -> Result<String, error::Error> {
        match self {
            parser::word::Parameter::Positional(p) => {
                if *p == 0 {
                    return Err(anyhow::anyhow!("unexpected positional parameter").into());
                }

                let parameter: &str =
                    if let Some(parameter) = shell.positional_parameters.get((p - 1) as usize) {
                        parameter
                    } else {
                        ""
                    };

                Ok(parameter.to_owned())
            }
            parser::word::Parameter::Special(s) => s.expand(shell).await,
            parser::word::Parameter::Named(n) => Ok(shell
                .env
                .get(n)
                .map_or_else(String::new, |v| String::from(&v.value))),
            parser::word::Parameter::NamedWithIndex { name, index } => match shell.env.get(name) {
                Some(ShellVariable { value, .. }) => Ok(value
                    .get_at(*index)?
                    .map_or_else(String::new, |s| s.to_owned())),
                None => Ok(String::new()),
            },
            parser::word::Parameter::NamedWithAllIndices { name, concatenate } => {
                match shell.env.get(name) {
                    Some(ShellVariable { value, .. }) => value.get_all(*concatenate),
                    None => Ok(String::new()),
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Expandable for parser::word::SpecialParameter {
    async fn expand(&self, shell: &mut Shell) -> Result<String, error::Error> {
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
                error::unimp("expansion: last background process id")
            }
            parser::word::SpecialParameter::ShellName => Ok(shell
                .shell_name
                .as_ref()
                .map_or_else(String::new, |name| name.clone())),
        }
    }
}

#[async_trait::async_trait]
impl Expandable for parser::ast::ArithmeticExpr {
    async fn expand(&self, shell: &mut Shell) -> Result<String, error::Error> {
        let value = self.eval(shell).await?;
        Ok(value.to_string())
    }
}
