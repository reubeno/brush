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

#[derive(Parser)]
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
    // N.B. These are skipped by clap, but filled in by the BuiltinDeclarationCommand trait.
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

        // TODO: implement declare -I
        if self.locals_inherit_from_prev_scope {
            log::error!("UNIMPLEMENTED: declare -I: locals inherit from previous scope");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        let mut result = BuiltinExitCode::Success;
        if !self.declarations.is_empty() {
            for declaration in &self.declarations {
                if self.print {
                    if !self.try_display_declaration(context, declaration, called_as_local)? {
                        result = BuiltinExitCode::Custom(1);
                    }
                } else {
                    if !self.process_declaration(context, declaration, called_as_local)? {
                        result = BuiltinExitCode::Custom(1);
                    }
                }
            }
        } else {
            // Display matching declarations from the variable environment.
            if !self.function_names_only && !self.function_names_or_defs_only {
                self.display_matching_env_declarations(context, called_as_local)?;
            }

            // Do the same for functions.
            if !called_as_local
                && (!self.print || self.function_names_only || self.function_names_or_defs_only)
            {
                self.display_matching_functions(context);
            }
        }

        Ok(result)
    }
}

impl DeclareCommand {
    fn try_display_declaration(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
        declaration: &CommandArg,
        called_as_local: bool,
    ) -> Result<bool, error::Error> {
        let name = match declaration {
            CommandArg::String(s) => s,
            CommandArg::Assignment(_) => {
                eprintln!("declare: {declaration}: not found");
                return Ok(false);
            }
        };

        let lookup = if called_as_local {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::OnlyInGlobal
        };

        if self.function_names_only || self.function_names_or_defs_only {
            if let Some(def) = context.shell.funcs.get(name) {
                if self.function_names_only {
                    if self.print {
                        println!("declare -f {name}");
                    } else {
                        println!("{name}");
                    }
                } else {
                    println!("{def}");
                }
                Ok(true)
            } else {
                eprintln!("declare: {name}: not found");
                Ok(false)
            }
        } else if let Some(variable) = context.shell.env.get_using_policy(name, lookup) {
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

            Ok(true)
        } else {
            eprintln!("declare: {name}: not found");
            Ok(false)
        }
    }

    fn process_declaration(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
        declaration: &CommandArg,
        called_as_local: bool,
    ) -> Result<bool, error::Error> {
        let create_var_local =
            called_as_local || (context.shell.in_function() && !self.create_global);

        if self.function_names_or_defs_only || self.function_names_only {
            return self.try_display_declaration(context, declaration, called_as_local);
        }

        // Extract the variable name and the initial value being assigned (if any).
        let (name, initial_value) = self.declaration_to_name_and_value(declaration)?;

        // Figure out where we should look.
        let lookup = if create_var_local {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::OnlyInGlobal
        };

        // Look up the variable.
        if let Some(var) = context
            .shell
            .env
            .get_mut_using_policy(name.as_str(), lookup)
        {
            if self.make_associative_array.is_some() {
                var.convert_to_associative_array()?;
            }
            if self.make_indexed_array.is_some() {
                var.convert_to_indexed_array()?;
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

            let scope = if create_var_local {
                EnvironmentScope::Local
            } else {
                EnvironmentScope::Global
            };

            context.shell.env.add(name, var, scope)?;
        }

        Ok(true)
    }

    #[allow(clippy::unused_self)]
    fn declaration_to_name_and_value(
        &self,
        declaration: &CommandArg,
    ) -> Result<(String, Option<ShellValueLiteral>), error::Error> {
        let name;
        let initial_value;

        match declaration {
            CommandArg::String(s) => {
                name = s.clone();
                initial_value = None;
            }
            CommandArg::Assignment(assignment) => {
                let assigned_index;

                match &assignment.name {
                    parser::ast::AssignmentName::VariableName(var_name) => {
                        name = var_name.to_owned();
                        assigned_index = None;
                    }
                    parser::ast::AssignmentName::ArrayElementName(var_name, index) => {
                        if matches!(assignment.value, parser::ast::AssignmentValue::Array(_)) {
                            return Err(error::Error::AssigningListToArrayMember);
                        }

                        name = var_name.to_owned();
                        assigned_index = Some(index.to_owned());
                    }
                }

                match &assignment.value {
                    parser::ast::AssignmentValue::Scalar(s) => {
                        if let Some(index) = assigned_index {
                            initial_value = Some(ShellValueLiteral::Array(ArrayLiteral(vec![(
                                Some(index),
                                s.value.clone(),
                            )])));
                        } else {
                            initial_value = Some(ShellValueLiteral::Scalar(s.value.clone()));
                        }
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

        Ok((name, initial_value))
    }

    fn display_matching_env_declarations(
        &self,
        context: &crate::builtin::BuiltinExecutionContext<'_>,
        called_as_local: bool,
    ) -> Result<(), error::Error> {
        //
        // Dump all declarations. Use attribute flags to filter which variables are dumped.
        //

        // We start by excluding all variables that are not enumerable.
        #[allow(clippy::type_complexity)]
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
        if let Some(value) = self.make_nameref.to_bool() {
            filters.push(Box::new(move |(_, v)| v.is_nameref() == value));
        }
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

        // Iterate through an ordered list of all matching declarations tracked in the
        // environment.
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

        Ok(())
    }

    fn display_matching_functions(&self, context: &crate::builtin::BuiltinExecutionContext<'_>) {
        for (name, def) in context.shell.funcs.iter().sorted_by_key(|v| v.0) {
            if self.function_names_only {
                println!("declare -f {name}");
            } else {
                println!("{def}");
            }
        }
    }

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
                var.unset_readonly()?;
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
    if variable.is_nameref() {
        result.push('n');
    }
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
