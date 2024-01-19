use std::collections::HashMap;

use anyhow::Result;
use clap::Parser;
use itertools::Itertools;

use crate::{
    builtin::{self, BuiltinCommand, BuiltinExitCode},
    env::{EnvironmentLookup, EnvironmentScope},
    error,
    variables::{self, ShellValue, ShellVariable, ShellVariableUpdateTransform},
};

builtin::minus_or_plus_flag_arg!(MakeIndexedArrayFlag, 'a', "");
builtin::minus_or_plus_flag_arg!(MakeAssociativeArrayFlag, 'A', "");
builtin::minus_or_plus_flag_arg!(MakeIntegerFlag, 'i', "");
builtin::minus_or_plus_flag_arg!(LowercaseValueOnAssignmentFlag, 'l', "");
builtin::minus_or_plus_flag_arg!(MakeNameRefFlag, 'n', "");
builtin::minus_or_plus_flag_arg!(MakeReadonlyFlag, 'r', "");
builtin::minus_or_plus_flag_arg!(MakeTracedFlag, 't', "");
builtin::minus_or_plus_flag_arg!(UppercaseValueOnAssignmentFlag, 'u', "");
builtin::minus_or_plus_flag_arg!(MakeExportedFlag, 'x', "");

#[derive(Parser, Debug)]
pub(crate) struct DeclareCommand {
    #[arg(short = 'f')]
    function_names_or_defs_only: bool,

    #[arg(short = 'F')]
    function_names_only: bool,

    #[arg(short = 'g')]
    create_global: bool,

    #[arg(short = 'I')]
    locals_inherit_from_prev_scope: bool,

    #[arg(short = 'p')]
    print: bool,

    //
    // Attribute options
    //
    #[clap(flatten)] // -a
    make_indexed_array: MakeIndexedArrayFlag,
    #[clap(flatten)] // -A
    make_associative_array: MakeAssociativeArrayFlag,
    #[clap(flatten)] // -i
    make_integer: MakeIntegerFlag,
    #[clap(flatten)] // -l
    lowercase_value_on_assignment: LowercaseValueOnAssignmentFlag,
    #[clap(flatten)] // -n
    make_nameref: MakeNameRefFlag,
    #[clap(flatten)] // -r
    make_readonly: MakeReadonlyFlag,
    #[clap(flatten)] // -t
    make_traced: MakeTracedFlag,
    #[clap(flatten)] // -u
    uppercase_value_on_assignment: UppercaseValueOnAssignmentFlag,
    #[clap(flatten)] // -x
    make_exported: MakeExportedFlag,

    //
    // Names
    //
    #[arg(name = "name[=value]")]
    names: Vec<String>,
}

#[allow(clippy::too_many_lines)]
#[async_trait::async_trait]
impl BuiltinCommand for DeclareCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        let called_as_local = context.builtin_name == "local";
        let create_var_local =
            called_as_local || (context.shell.in_function() && !self.create_global);

        // Note that we don't implement much.
        if self.locals_inherit_from_prev_scope {
            log::error!("UNIMPLEMENTED: declare -I: locals inherit from previous scope");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        let mut result = BuiltinExitCode::Success;
        if !self.names.is_empty() {
            for entry in &self.names {
                if self.print {
                    if self.function_names_only || self.function_names_or_defs_only {
                        log::error!(
                            "UNIMPLEMENTED: declare -p: function names or definitions only"
                        );
                        return Ok(BuiltinExitCode::Unimplemented);
                    } else if let Some(variable) = context.shell.env.get(entry) {
                        let cs = get_declare_flag_str(variable);
                        println!(
                            "declare -{cs} {entry}={}",
                            variable
                                .value
                                .format(variables::FormatStyle::DeclarePrint)?
                        );
                    } else {
                        eprintln!("declare: {entry}: not found");
                        result = BuiltinExitCode::Custom(1);
                    }

                    continue;
                }

                if self.function_names_or_defs_only {
                    log::error!("UNIMPLEMENTED: declare -f: function names or definitions only");
                    return Ok(BuiltinExitCode::Unimplemented);
                }
                if self.function_names_only {
                    log::error!("UNIMPLEMENTED: declare -F: function names only");
                    return Ok(BuiltinExitCode::Unimplemented);
                }

                let (name, value) = entry.split_once('=').map_or_else(
                    || (entry.as_str(), None),
                    |(name, value)| (name, Some(value)),
                );

                let lookup = if create_var_local {
                    EnvironmentLookup::OnlyInCurrentLocal
                } else {
                    EnvironmentLookup::OnlyInGlobal
                };

                let scope = if create_var_local {
                    EnvironmentScope::Local
                } else {
                    EnvironmentScope::Global
                };

                // TODO
                result = BuiltinExitCode::Unimplemented;

                if let Some(var) = context.shell.env.get_mut_using_policy(name, lookup) {
                    if self.make_indexed_array.is_some() {
                        log::error!("UNIMPLEMENTED: declare -a: converting to indexed array");
                        return Ok(BuiltinExitCode::Unimplemented);
                    }
                    if self.make_associative_array.is_some() {
                        log::error!("UNIMPLEMENTED: declare -A: converting to associative array");
                        return Ok(BuiltinExitCode::Unimplemented);
                    }

                    // TODO: handle setting the attributes *before* the new assignment.
                    if let Some(value) = value {
                        var.set_by_str(value)?;
                    }

                    self.apply_attributes(var)?;
                } else {
                    let initial_value = if self.make_indexed_array.is_some() {
                        if let Some(value) = value {
                            ShellValue::new_indexed_array(value)
                        } else {
                            ShellValue::IndexedArray(vec![])
                        }
                    } else if self.make_associative_array.is_some() {
                        if let Some(value) = value {
                            ShellValue::new_associative_array(value)
                        } else {
                            ShellValue::AssociativeArray(HashMap::new())
                        }
                    } else {
                        value.unwrap_or("").into()
                    };

                    // TODO: handle declaring without value for variable of different type.
                    // TODO: handle setting the attributes *before* the first assignment.
                    context.shell.env.update_or_add(
                        name,
                        initial_value,
                        |v| self.apply_attributes(v),
                        lookup,
                        scope,
                    )?;
                }
            }
        } else {
            //
            // Dump variables. Use attribute flags to filter which variables are dumped.
            //
            // TODO: Figure out scoping?
            //

            // We start by excluding all variables that are not enumerable.
            let mut filters: Vec<Box<dyn Fn((&String, &ShellVariable)) -> bool>> =
                vec![Box::new(|(_, v)| v.enumerable)];

            // Add filters depending on attribute flags.
            if let Some(value) = self.make_indexed_array.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(v.value, ShellValue::IndexedArray(_)) == value
                }));
            }
            if let Some(value) = self.make_associative_array.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(v.value, ShellValue::AssociativeArray(_)) == value
                }));
            }
            if let Some(value) = self.make_integer.to_bool() {
                filters.push(Box::new(move |(_, v)| v.treat_as_integer == value));
            }
            if let Some(value) = self.lowercase_value_on_assignment.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(
                        v.transform_on_update,
                        ShellVariableUpdateTransform::Lowercase
                    ) == value
                }));
            }
            // TODO: nameref
            if let Some(value) = self.make_readonly.to_bool() {
                filters.push(Box::new(move |(_, v)| v.readonly == value));
            }
            if let Some(value) = self.make_readonly.to_bool() {
                filters.push(Box::new(move |(_, v)| v.trace == value));
            }
            if let Some(value) = self.uppercase_value_on_assignment.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(
                        v.transform_on_update,
                        ShellVariableUpdateTransform::Uppercase
                    ) == value
                }));
            }
            if let Some(value) = self.make_exported.to_bool() {
                filters.push(Box::new(move |(_, v)| v.exported == value));
            }

            for (name, variable) in context
                .shell
                .env
                .iter()
                .filter(|pair| filters.iter().all(|f| f(*pair)))
                .sorted_by_key(|v| v.0)
            {
                if self.print {
                    let cs = get_declare_flag_str(variable);
                    println!(
                        "declare -{cs} {name}={}",
                        variable
                            .value
                            .format(variables::FormatStyle::DeclarePrint)?
                    );
                } else {
                    println!(
                        "{name}={}",
                        variable.value.format(variables::FormatStyle::Basic)?
                    );
                }
            }

            // TODO: dump functions
        }

        Ok(result)
    }
}

impl DeclareCommand {
    fn apply_attributes(&self, var: &mut ShellVariable) -> Result<(), error::Error> {
        if let Some(value) = self.make_integer.to_bool() {
            var.treat_as_integer = value;
        }
        if let Some(value) = self.lowercase_value_on_assignment.to_bool() {
            if value {
                var.transform_on_update = ShellVariableUpdateTransform::Lowercase;
            } else if matches!(
                var.transform_on_update,
                ShellVariableUpdateTransform::Lowercase
            ) {
                var.transform_on_update = ShellVariableUpdateTransform::None;
            }
        }
        if let Some(value) = self.make_nameref.to_bool() {
            if value {
                log::error!("UNIMPLEMENTED: declare -n: make nameref");
                return Err(error::Error::Unimplemented("declare with nameref"));
            }
        }
        if let Some(value) = self.make_readonly.to_bool() {
            var.readonly = value;
        }
        if let Some(value) = self.make_traced.to_bool() {
            var.trace = value;
        }
        if let Some(value) = self.uppercase_value_on_assignment.to_bool() {
            if value {
                var.transform_on_update = ShellVariableUpdateTransform::Uppercase;
            } else if matches!(
                var.transform_on_update,
                ShellVariableUpdateTransform::Uppercase
            ) {
                var.transform_on_update = ShellVariableUpdateTransform::None;
            }
        }
        if let Some(value) = self.make_exported.to_bool() {
            var.exported = value;
        }

        Ok(())
    }
}

fn get_declare_flag_str(variable: &ShellVariable) -> String {
    let mut result = String::new();

    if matches!(variable.value, ShellValue::IndexedArray(_)) {
        result.push('a');
    }
    if matches!(variable.value, ShellValue::AssociativeArray(_)) {
        result.push('A');
    }
    if variable.treat_as_integer {
        result.push('i');
    }
    if let ShellVariableUpdateTransform::Lowercase = variable.transform_on_update {
        result.push('l');
    }
    // TODO: nameref
    if variable.readonly {
        result.push('r');
    }
    if variable.trace {
        result.push('t');
    }
    if let ShellVariableUpdateTransform::Uppercase = variable.transform_on_update {
        result.push('u');
    }
    if variable.exported {
        result.push('x');
    }

    if result.is_empty() {
        result.push('-');
    }

    result
}
