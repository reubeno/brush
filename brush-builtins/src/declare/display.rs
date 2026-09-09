//! The half of the declaration builtins that reads the environment rather than changing it:
//! listing variables and functions, and the `declare -p` line each is displayed as.

use itertools::Itertools;
use std::io::Write;

use brush_core::{
    env::EnvironmentLookup,
    variables::{self, ShellValue, ShellVariable, ShellVariableUpdateTransform},
};

use super::{DeclareCommand, DeclareVerb};

impl DeclareCommand {
    /// Displays the variable or function named by an operand. Returns `true` if it was found.
    pub(super) fn try_display_declaration(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, brush_core::Error> {
        let name = match declaration {
            brush_core::CommandArg::String(s) => s,
            brush_core::CommandArg::Assignment(assignment) => {
                writeln!(
                    context.stderr(),
                    "{}: {assignment}: not found",
                    context.command_name
                )?;
                return Ok(false);
            }
        };

        let lookup = if matches!(verb, DeclareVerb::Local) {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        if self.function_names_only || self.function_names_or_defs_only {
            if let Some(func_registration) = context.shell.funcs().get(name) {
                if self.function_names_only {
                    if self.print {
                        writeln!(
                            context.stdout(),
                            "declare -{} {name}",
                            func_registration.attribute_flags()
                        )?;
                    } else {
                        writeln!(context.stdout(), "{name}")?;
                    }
                } else {
                    writeln!(context.stdout(), "{}", func_registration.definition())?;
                }
                Ok(true)
            } else {
                // A shell reports a missing function only through the exit status here.
                Ok(false)
            }
        } else if let Some(variable) = context.shell.env().get_using_policy(name, lookup) {
            let resolved_value = variable.resolve_value(context.shell);
            write_declare_line(context, name, variable, &resolved_value)?;
            Ok(true)
        } else {
            // Diagnostics name the builtin as invoked (`local`, `typeset`, ...), even though
            // displayed declarations always read `declare`.
            writeln!(
                context.stderr(),
                "{}: {name}: not found",
                context.command_name
            )?;
            Ok(false)
        }
    }

    /// Returns the predicates the attribute options select variables with, one per option given
    /// in its `-X` form. They apply as a union: `declare -rt` lists variables that are readonly
    /// *or* traced. A plus option (`+x`) selects nothing, as in a shell.
    pub(super) fn attribute_selectors(&self) -> Vec<VariableSelector> {
        let mut selectors: Vec<VariableSelector> = vec![];
        if self.make_indexed_array.to_bool() == Some(true) {
            selectors.push(|v| v.value().is_indexed_array());
        }
        if self.make_associative_array.to_bool() == Some(true) {
            selectors.push(|v| v.value().is_associative_array());
        }
        if self.make_integer.to_bool() == Some(true) {
            selectors.push(|v| v.is_treated_as_integer());
        }
        if self.capitalize_value_on_assignment.to_bool() == Some(true) {
            selectors.push(|v| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Capitalize
                )
            });
        }
        if self.lowercase_value_on_assignment.to_bool() == Some(true) {
            selectors.push(|v| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Lowercase
                )
            });
        }
        if self.make_nameref.to_bool() == Some(true) {
            selectors.push(|v| v.is_treated_as_nameref());
        }
        if self.make_readonly.to_bool() == Some(true) {
            selectors.push(|v| v.is_readonly());
        }
        if self.make_traced.to_bool() == Some(true) {
            selectors.push(|v| v.is_trace_enabled());
        }
        if self.uppercase_value_on_assignment.to_bool() == Some(true) {
            selectors.push(|v| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Uppercase
                )
            });
        }
        if self.make_exported.to_bool() == Some(true) {
            selectors.push(|v| v.is_exported());
        }
        selectors
    }

    /// Displays all variables whose attributes match the requested filters.
    pub(super) fn display_matching_env_declarations(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        verb: DeclareVerb,
    ) -> Result<(), brush_core::Error> {
        // The verb decides which variables are eligible at all: `readonly` and `export` list
        // only the variables carrying their attribute. Attribute options then select among them.
        let eligible = |v: &ShellVariable| {
            v.is_enumerable()
                && match verb {
                    DeclareVerb::Readonly => v.is_readonly(),
                    DeclareVerb::Export => v.is_exported(),
                    DeclareVerb::Declare | DeclareVerb::Local => true,
                }
        };
        let selectors = self.attribute_selectors();

        // A shell lists in `declare -p` form whenever an attribute option or an
        // attribute-implying verb selected the variables, not only under `-p`.
        let declare_form = self.print || verb.implies_attribute() || !selectors.is_empty();

        let iter_policy = if matches!(verb, DeclareVerb::Local) {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        for (name, variable) in context
            .shell
            .env()
            .iter_using_policy(iter_policy)
            .filter(|(_, v)| {
                eligible(v) && (selectors.is_empty() || selectors.iter().any(|f| f(v)))
            })
            .sorted_by_key(|v| v.0)
        {
            if declare_form {
                write_declare_line(context, name, variable, variable.value())?;
            } else {
                writeln!(
                    context.stdout(),
                    "{name}={}",
                    variable
                        .value()
                        .format(variables::FormatStyle::Basic, context.shell)?
                )?;
            }
        }

        Ok(())
    }

    /// Displays shell functions. An attribute option (`-x`, `-r`, `-t`) or an
    /// attribute-implying verb lists only the functions carrying one of those attributes, each
    /// definition followed by its attribute line.
    pub(super) fn display_matching_functions(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        verb: DeclareVerb,
    ) -> Result<(), brush_core::Error> {
        use brush_core::functions::Registration;

        let mut selectors: Vec<fn(&Registration) -> bool> = vec![];
        match verb {
            DeclareVerb::Export => selectors.push(Registration::is_exported),
            DeclareVerb::Readonly => selectors.push(Registration::is_readonly),
            DeclareVerb::Declare | DeclareVerb::Local => (),
        }
        if self.make_exported.to_bool() == Some(true) {
            selectors.push(Registration::is_exported);
        }
        if self.make_readonly.to_bool() == Some(true) {
            selectors.push(Registration::is_readonly);
        }
        if self.make_traced.to_bool() == Some(true) {
            selectors.push(Registration::is_trace_enabled);
        }
        let filtered = !selectors.is_empty();

        for (name, registration) in context
            .shell
            .funcs()
            .iter()
            .filter(|(_, registration)| !filtered || selectors.iter().any(|s| s(registration)))
            .sorted_by_key(|v| v.0)
        {
            if !self.function_names_only {
                writeln!(context.stdout(), "{}", registration.definition())?;
            }
            if self.function_names_only || filtered {
                writeln!(
                    context.stdout(),
                    "declare -{} {name}",
                    registration.attribute_flags()
                )?;
            }
        }

        Ok(())
    }
}

/// A predicate selecting variables for display.
type VariableSelector = fn(&ShellVariable) -> bool;

/// Writes the `declare -<flags> name=value` line that displays a variable.
///
/// # Arguments
///
/// * `value` - The value to display; a dynamic variable's already resolved, if the caller wants
///   it shown.
fn write_declare_line(
    context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    name: &str,
    variable: &ShellVariable,
    value: &ShellValue,
) -> Result<(), brush_core::Error> {
    let mut flags = variable.attribute_flags(context.shell);
    if flags.is_empty() {
        flags.push('-');
    }

    let separator = if matches!(value, ShellValue::Unset(_)) {
        ""
    } else {
        "="
    };

    writeln!(
        context.stdout(),
        "declare -{flags} {name}{separator}{}",
        value.format(variables::FormatStyle::DeclarePrint, context.shell)?
    )?;

    Ok(())
}
