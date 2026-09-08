use std::io::Write;

use clap::Parser;

use brush_core::{ExecutionParameters, ExecutionResult, Shell, builtins, variables::ArrayKind};

/// Unset a variable.
#[derive(Parser)]
pub(crate) struct UnsetCommand {
    #[clap(flatten)]
    name_interpretation: UnsetNameInterpretation,

    /// Names of variables to unset.
    names: Vec<String>,
}

#[derive(Parser)]
#[clap(group = clap::ArgGroup::new("name-interpretation").multiple(false).required(false))]
pub(crate) struct UnsetNameInterpretation {
    /// Treat each name as a shell function.
    #[arg(short = 'f', group = "name-interpretation")]
    shell_functions: bool,

    /// Treat each name as a shell variable.
    #[arg(short = 'v', group = "name-interpretation")]
    shell_variables: bool,

    /// Treat each name as a name reference.
    #[arg(short = 'n', group = "name-interpretation")]
    name_references: bool,
}

impl UnsetNameInterpretation {
    pub const fn unspecified(&self) -> bool {
        !self.shell_functions && !self.shell_variables && !self.name_references
    }
}

impl builtins::Command for UnsetCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        //
        // TODO(nameref): implement nameref
        //
        if self.name_interpretation.name_references {
            return brush_core::error::unimp("unset: name references are not yet implemented");
        }

        let unspecified = self.name_interpretation.unspecified();
        let mut result = ExecutionResult::success();

        #[expect(clippy::needless_continue)]
        for name in &self.names {
            if unspecified || self.name_interpretation.shell_variables {
                // Try to parse the name as a parameter. If we can't, don't bail; it may not be a
                // valid variable name/parameter but could still be a function name.
                if let Ok(parameter) =
                    brush_parser::word::parse_parameter(name, &context.shell.parser_options())
                {
                    // The diagnostic below names the variable, not the operand, so an element's
                    // base name is kept alongside the outcome.
                    let (target, removed) = match parameter {
                        brush_parser::word::Parameter::Positional(_) => continue,
                        brush_parser::word::Parameter::Special(_) => continue,
                        brush_parser::word::Parameter::Named(name) => {
                            let removed = context.shell.env_mut().unset(name.as_str());
                            (name, removed.map(|prev| prev.is_some()))
                        }
                        brush_parser::word::Parameter::NamedWithIndex { name, index } => {
                            let removed = unset_array_element(
                                context.shell,
                                &context.params,
                                name.as_str(),
                                index.as_str(),
                            )
                            .await;
                            (name, removed)
                        }
                        // `name[*]` and `name[@]` reach the word parser as their own parameter
                        // kind, but `unset` reads them as ordinary subscripts.
                        brush_parser::word::Parameter::NamedWithAllIndices {
                            name,
                            concatenate,
                        } => {
                            let index = if concatenate { "*" } else { "@" };
                            let removed = unset_array_element(
                                context.shell,
                                &context.params,
                                name.as_str(),
                                index,
                            )
                            .await;
                            (name, removed)
                        }
                    };

                    match removed {
                        Ok(true) => continue,
                        Ok(false) => (),
                        // A readonly variable stays, whether the operand named the whole
                        // variable or one of its elements; the remaining names are still
                        // processed.
                        Err(err)
                            if matches!(err.kind(), brush_core::ErrorKind::ReadonlyVariable) =>
                        {
                            writeln!(
                                context.stderr(),
                                "{}: {target}: cannot unset: readonly variable",
                                context.command_name
                            )?;
                            result = ExecutionResult::general_error();
                            continue;
                        }
                        // A subscript on something that is not an array fails this name, but
                        // the remaining names are still processed.
                        Err(err) if matches!(err.kind(), brush_core::ErrorKind::NotArray) => {
                            writeln!(
                                context.stderr(),
                                "{}: {target}: not an array variable",
                                context.command_name
                            )?;
                            result = ExecutionResult::general_error();
                            continue;
                        }
                        Err(err) => return Err(err),
                    }
                }
            }

            if unspecified || self.name_interpretation.shell_functions {
                match context.shell.undefine_func(name) {
                    Ok(true) => continue,
                    Ok(false) => (),
                    // A readonly function stays; the remaining names are still processed.
                    Err(err)
                        if matches!(err.kind(), brush_core::ErrorKind::ReadonlyFunction(_)) =>
                    {
                        writeln!(
                            context.stderr(),
                            "{}: {name}: cannot unset: readonly function",
                            context.command_name
                        )?;
                        result = ExecutionResult::general_error();
                    }
                    Err(err) => return Err(err),
                }
            }
        }

        Ok(result)
    }
}

/// Unsets the element a `name[subscript]` operand names. Returns whether anything was removed.
///
/// The subscript is resolved the way every other subscript is -- see
/// [`brush_core::Shell::resolve_array_subscript`] -- with the outcomes a shell reserves for
/// `unset` layered on top: an empty subscript names nothing, `*` and `@` name every element, and
/// a variable that is not an array behaves as if it were element 0 of itself.
async fn unset_array_element(
    shell: &mut Shell<impl brush_core::ShellExtensions>,
    params: &ExecutionParameters,
    name: &str,
    index: &str,
) -> Result<bool, brush_core::Error> {
    let Some((_, var)) = shell.env().get(name) else {
        return Ok(false);
    };

    // An empty subscript names no element at all; a shell ignores it silently, and does so
    // before it would refuse a readonly variable.
    if index.is_empty() {
        return Ok(true);
    }
    if var.is_readonly() {
        return Err(brush_core::ErrorKind::ReadonlyVariable.into());
    }

    let kind = var.value().array_kind();
    if matches!(index, "*" | "@") {
        return shell.env_mut().unset_all_indices(name);
    }

    let Some(kind) = kind else {
        // A variable that is not an array still answers a subscript that evaluates to 0: it is
        // its own element 0, and unsetting that unsets the variable.
        let index = shell
            .resolve_array_subscript(params, index, ArrayKind::Indexed)
            .await?;
        if index != "0" {
            return Err(brush_core::ErrorKind::NotArray.into());
        }
        return Ok(shell.env_mut().unset(name)?.is_some());
    };

    let index = shell.resolve_array_subscript(params, index, kind).await?;
    shell.env_mut().unset_index(name, index.as_str())
}
