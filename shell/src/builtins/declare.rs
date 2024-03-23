use anyhow::Result;
use clap::Parser;
use itertools::Itertools;

use crate::{
    builtin::{self, BuiltinCommand, BuiltinDeclarationCommand, BuiltinExitCode},
    commands::CommandArg,
    env::{EnvironmentLookup, EnvironmentScope},
    error,
    variables::{
        self, ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
        ShellVariableUpdateTransform,
    },
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
    /// Constrain to function names or definitions.
    #[arg(short = 'f')]
    function_names_or_defs_only: bool,

    /// Constrain to function names only.
    #[arg(short = 'F')]
    function_names_only: bool,

    /// Create global variable, if applicable.
    #[arg(short = 'g')]
    create_global: bool,

    #[arg(short = 'I')]
    locals_inherit_from_prev_scope: bool,

    /// Display each item's attributes and values.
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
    // Declarations
    //
    #[clap(skip)]
    declarations: Vec<CommandArg>,
}

impl BuiltinDeclarationCommand for DeclareCommand {
    fn set_declarations(&mut self, declarations: Vec<CommandArg>) {
        self.declarations = declarations;
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait::async_trait]
impl BuiltinCommand for DeclareCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let called_as_local = context.builtin_name == "local";
        let create_var_local =
            called_as_local || (context.shell.in_function() && !self.create_global);

        // Note that we don't implement much.
        if self.locals_inherit_from_prev_scope {
            log::error!("UNIMPLEMENTED: declare -I: locals inherit from previous scope");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        let mut result = BuiltinExitCode::Success;
        if !self.declarations.is_empty() {
            for declaration in &self.declarations {
                if self.print {
                    let name = match declaration {
                        CommandArg::String(s) => s,
                        CommandArg::Assignment(assignment) => match &assignment.name {
                            parser::ast::AssignmentName::VariableName(name) => name,
                            parser::ast::AssignmentName::ArrayElementName(_, _) => {
                                return error::unimp("declare -p with array index");
                            }
                        },
                    };

                    if self.function_names_only || self.function_names_or_defs_only {
                        log::error!(
                            "UNIMPLEMENTED: declare -p: function names or definitions only"
                        );
                        return Ok(BuiltinExitCode::Unimplemented);
                    } else if let Some(variable) = context.shell.env.get(name) {
                        let cs = get_declare_flag_str(variable);
                        let separator_str = if matches!(variable.value(), ShellValue::Unset(_)) {
                            ""
                        } else {
                            "="
                        };

                        println!(
                            "declare -{cs} {name}{separator_str}{}",
                            variable
                                .value()
                                .format(variables::FormatStyle::DeclarePrint)?
                        );
                    } else {
                        eprintln!("declare: {name}: not found");
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

                let name;
                let initial_value;

                match declaration {
                    CommandArg::String(s) => {
                        name = s.clone();
                        initial_value = None;
                    }
                    CommandArg::Assignment(assignment) => {
                        match &assignment.name {
                            parser::ast::AssignmentName::VariableName(var_name) => {
                                name = var_name.to_owned();
                            }
                            parser::ast::AssignmentName::ArrayElementName(_, _) => {
                                return error::unimp("declaring array index");
                            }
                        }

                        match &assignment.value {
                            parser::ast::AssignmentValue::Scalar(s) => {
                                initial_value = Some(ShellValueLiteral::Scalar(s.value.clone()));
                            }
                            parser::ast::AssignmentValue::Array(a) => {
                                initial_value = Some(ShellValueLiteral::Array(ArrayLiteral(
                                    a.iter()
                                        .map(|(i, v)| {
                                            (i.as_ref().map(|w| w.value.clone()), v.value.clone())
                                        })
                                        .collect(),
                                )));
                            }
                        }
                    }
                }

                // TODO
                result = BuiltinExitCode::Unimplemented;

                if let Some(var) = context
                    .shell
                    .env
                    .get_mut_using_policy(name.as_str(), lookup)
                {
                    if self.make_indexed_array.is_some() {
                        log::error!("UNIMPLEMENTED: declare -a: converting to indexed array");
                        return Ok(BuiltinExitCode::Unimplemented);
                    }
                    if self.make_associative_array.is_some() {
                        log::error!("UNIMPLEMENTED: declare -A: converting to associative array");
                        return Ok(BuiltinExitCode::Unimplemented);
                    }

                    self.apply_attributes_before_update(var)?;

                    if let Some(initial_value) = initial_value {
                        var.assign(initial_value, false)?;
                    }

                    self.apply_attributes_after_update(var)?;
                } else {
                    let unset_type = if self.make_indexed_array.is_some() {
                        ShellValueUnsetType::IndexedArray
                    } else if self.make_associative_array.is_some() {
                        ShellValueUnsetType::AssociativeArray
                    } else {
                        ShellValueUnsetType::Untyped
                    };

                    let mut var = ShellVariable::new(ShellValue::Unset(unset_type));

                    self.apply_attributes_before_update(&mut var)?;

                    if let Some(initial_value) = initial_value {
                        var.assign(initial_value, false)?;
                    }

                    self.apply_attributes_after_update(&mut var)?;

                    context.shell.env.add(name, var, scope)?;
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
                vec![Box::new(|(_, v)| v.is_enumerable())];

            // Add filters depending on attribute flags.
            if let Some(value) = self.make_indexed_array.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(v.value(), ShellValue::IndexedArray(_)) == value
                }));
            }
            if let Some(value) = self.make_associative_array.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(v.value(), ShellValue::AssociativeArray(_)) == value
                }));
            }
            if let Some(value) = self.make_integer.to_bool() {
                filters.push(Box::new(move |(_, v)| v.is_treated_as_integer() == value));
            }
            if let Some(value) = self.lowercase_value_on_assignment.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(
                        v.get_update_transform(),
                        ShellVariableUpdateTransform::Lowercase
                    ) == value
                }));
            }
            // TODO: nameref
            if let Some(value) = self.make_readonly.to_bool() {
                filters.push(Box::new(move |(_, v)| v.is_readonly() == value));
            }
            if let Some(value) = self.make_readonly.to_bool() {
                filters.push(Box::new(move |(_, v)| v.is_trace_enabled() == value));
            }
            if let Some(value) = self.uppercase_value_on_assignment.to_bool() {
                filters.push(Box::new(move |(_, v)| {
                    matches!(
                        v.get_update_transform(),
                        ShellVariableUpdateTransform::Uppercase
                    ) == value
                }));
            }
            if let Some(value) = self.make_exported.to_bool() {
                filters.push(Box::new(move |(_, v)| v.is_exported() == value));
            }

            let iter_policy = if called_as_local {
                EnvironmentLookup::OnlyInCurrentLocal
            } else {
                EnvironmentLookup::Anywhere
            };

            for (name, variable) in context
                .shell
                .env
                .iter_using_policy(iter_policy)
                .filter(|pair| filters.iter().all(|f| f(*pair)))
                .sorted_by_key(|v| v.0)
            {
                if self.print {
                    let cs = get_declare_flag_str(variable);
                    let separator_str = if matches!(variable.value(), ShellValue::Unset(_)) {
                        ""
                    } else {
                        "="
                    };

                    println!(
                        "declare -{cs} {name}{separator_str}{}",
                        variable
                            .value()
                            .format(variables::FormatStyle::DeclarePrint)?
                    );
                } else {
                    println!(
                        "{name}={}",
                        variable.value().format(variables::FormatStyle::Basic)?
                    );
                }
            }

            // TODO: dump functions
        }

        Ok(result)
    }
}

impl DeclareCommand {
    fn apply_attributes_before_update(&self, var: &mut ShellVariable) -> Result<(), error::Error> {
        if let Some(value) = self.make_integer.to_bool() {
            if value {
                var.treat_as_integer();
            } else {
                var.unset_treat_as_integer();
            }
        }
        if let Some(value) = self.lowercase_value_on_assignment.to_bool() {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Lowercase);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Lowercase
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.make_nameref.to_bool() {
            if value {
                log::error!("UNIMPLEMENTED: declare -n: make nameref");
                return Err(error::Error::Unimplemented("declare with nameref"));
            }
        }
        if let Some(value) = self.make_traced.to_bool() {
            if value {
                var.enable_trace();
            } else {
                var.disable_trace();
            }
        }
        if let Some(value) = self.uppercase_value_on_assignment.to_bool() {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Uppercase);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Uppercase
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.make_exported.to_bool() {
            if value {
                var.export();
            } else {
                var.unexport();
            }
        }

        Ok(())
    }

    #[allow(clippy::unnecessary_wraps)]
    fn apply_attributes_after_update(&self, var: &mut ShellVariable) -> Result<(), error::Error> {
        if let Some(value) = self.make_readonly.to_bool() {
            if value {
                var.set_readonly();
            } else {
                var.unset_readonly();
            }
        }

        Ok(())
    }
}

fn get_declare_flag_str(variable: &ShellVariable) -> String {
    let mut result = String::new();

    if matches!(
        variable.value(),
        ShellValue::IndexedArray(_) | ShellValue::Unset(ShellValueUnsetType::IndexedArray)
    ) {
        result.push('a');
    }
    if matches!(
        variable.value(),
        ShellValue::AssociativeArray(_) | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
    ) {
        result.push('A');
    }
    if variable.is_treated_as_integer() {
        result.push('i');
    }
    // TODO: nameref
    if variable.is_readonly() {
        result.push('r');
    }
    if let ShellVariableUpdateTransform::Lowercase = variable.get_update_transform() {
        result.push('l');
    }
    if variable.is_trace_enabled() {
        result.push('t');
    }
    if let ShellVariableUpdateTransform::Uppercase = variable.get_update_transform() {
        result.push('u');
    }
    if variable.is_exported() {
        result.push('x');
    }

    if result.is_empty() {
        result.push('-');
    }

    result
}
