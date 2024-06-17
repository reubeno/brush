use clap::Parser;
use itertools::Itertools;
use std::io::Write;

use crate::{
    builtin, commands,
    env::{EnvironmentLookup, EnvironmentScope},
    error,
    variables::{
        self, ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
        ShellVariableUpdateTransform,
    },
};

builtin::minus_or_plus_flag_arg!(
    MakeIndexedArrayFlag,
    'a',
    "Make the variable an indexed array."
);
builtin::minus_or_plus_flag_arg!(
    MakeAssociativeArrayFlag,
    'A',
    "Make the variable an associative array."
);
builtin::minus_or_plus_flag_arg!(MakeIntegerFlag, 'i', "Mark the variable as integer-typed");
builtin::minus_or_plus_flag_arg!(
    LowercaseValueOnAssignmentFlag,
    'l',
    "Enable lowercase-on-assignment for the variable."
);
builtin::minus_or_plus_flag_arg!(
    MakeNameRefFlag,
    'n',
    "Mark the variable as a name reference"
);
builtin::minus_or_plus_flag_arg!(MakeReadonlyFlag, 'r', "Mark the variable as read-only.");
builtin::minus_or_plus_flag_arg!(MakeTracedFlag, 't', "Enable tracing for the variable.");
builtin::minus_or_plus_flag_arg!(
    UppercaseValueOnAssignmentFlag,
    'u',
    "Enable uppercase-on-assignment for the variable."
);
builtin::minus_or_plus_flag_arg!(MakeExportedFlag, 'x', "Mark the variable for export.");

/// Display or update variables and their attributes.
#[derive(Parser)]
#[clap(override_usage = "declare [OPTIONS] [DECLARATIONS]...")]
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

    /// When creating a local variable that shadows another variable of the same name,
    /// then initialize it with the contents and attributes of the variable being shadowed.
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
    declarations: Vec<commands::CommandArg>,
}

#[derive(Clone, Copy)]
enum DeclareVerb {
    Declare,
    Local,
    Readonly,
}

impl builtin::DeclarationCommand for DeclareCommand {
    fn set_declarations(&mut self, declarations: Vec<commands::CommandArg>) {
        self.declarations = declarations;
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait::async_trait]
impl builtin::Command for DeclareCommand {
    fn takes_plus_options() -> bool {
        true
    }

    async fn execute(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        let verb = match context.command_name.as_str() {
            "local" => DeclareVerb::Local,
            "readonly" => DeclareVerb::Readonly,
            _ => DeclareVerb::Declare,
        };

        // TODO: implement declare -I
        if self.locals_inherit_from_prev_scope {
            writeln!(
                context.stderr(),
                "UNIMPLEMENTED: declare -I: locals inherit from previous scope"
            )?;
            return Ok(builtin::ExitCode::Unimplemented);
        }

        let mut result = builtin::ExitCode::Success;
        if !self.declarations.is_empty() {
            for declaration in &self.declarations {
                if self.print && !matches!(verb, DeclareVerb::Readonly) {
                    if !self.try_display_declaration(&mut context, declaration, verb)? {
                        result = builtin::ExitCode::Custom(1);
                    }
                } else {
                    if !self.process_declaration(&mut context, declaration, verb)? {
                        result = builtin::ExitCode::Custom(1);
                    }
                }
            }
        } else {
            // Display matching declarations from the variable environment.
            if !self.function_names_only && !self.function_names_or_defs_only {
                self.display_matching_env_declarations(&mut context, verb)?;
            }

            // Do the same for functions.
            if !matches!(verb, DeclareVerb::Local | DeclareVerb::Readonly)
                && (!self.print || self.function_names_only || self.function_names_or_defs_only)
            {
                self.display_matching_functions(&mut context)?;
            }
        }

        Ok(result)
    }
}

impl DeclareCommand {
    fn try_display_declaration(
        &self,
        context: &mut crate::commands::ExecutionContext<'_>,
        declaration: &commands::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, error::Error> {
        let name = match declaration {
            commands::CommandArg::String(s) => s,
            commands::CommandArg::Assignment(_) => {
                writeln!(context.stderr(), "declare: {declaration}: not found")?;
                return Ok(false);
            }
        };

        let lookup = if matches!(verb, DeclareVerb::Local) {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        if self.function_names_only || self.function_names_or_defs_only {
            if let Some(func_registration) = context.shell.funcs.get(name) {
                if self.function_names_only {
                    if self.print {
                        writeln!(context.stdout(), "declare -f {name}")?;
                    } else {
                        writeln!(context.stdout(), "{name}")?;
                    }
                } else {
                    writeln!(context.stdout(), "{}", func_registration.definition)?;
                }
                Ok(true)
            } else {
                writeln!(context.stderr(), "declare: {name}: not found")?;
                Ok(false)
            }
        } else if let Some(variable) = context.shell.env.get_using_policy(name, lookup) {
            let mut cs = variable.get_attribute_flags();
            if cs.is_empty() {
                cs.push('-');
            }

            let separator_str = if matches!(variable.value(), ShellValue::Unset(_)) {
                ""
            } else {
                "="
            };

            writeln!(
                context.stdout(),
                "declare -{cs} {name}{separator_str}{}",
                variable
                    .value()
                    .format(variables::FormatStyle::DeclarePrint)?
            )?;

            Ok(true)
        } else {
            writeln!(context.stderr(), "declare: {name}: not found")?;
            Ok(false)
        }
    }

    fn process_declaration(
        &self,
        context: &mut crate::commands::ExecutionContext<'_>,
        declaration: &commands::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, error::Error> {
        let create_var_local = matches!(verb, DeclareVerb::Local)
            || (context.shell.in_function() && !self.create_global);

        if self.function_names_or_defs_only || self.function_names_only {
            return self.try_display_declaration(context, declaration, verb);
        }

        // Extract the variable name and the initial value being assigned (if any).
        let (name, assigned_index, initial_value, name_is_array) =
            Self::declaration_to_name_and_value(declaration)?;

        // Figure out where we should look.
        let lookup = if create_var_local {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
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
                // We append if the declaration included an explicit index.
                var.assign(initial_value, assigned_index.is_some())?;
            }

            self.apply_attributes_after_update(var, verb)?;
        } else {
            let unset_type = if self.make_indexed_array.is_some() {
                ShellValueUnsetType::IndexedArray
            } else if self.make_associative_array.is_some() {
                ShellValueUnsetType::AssociativeArray
            } else if name_is_array {
                ShellValueUnsetType::IndexedArray
            } else {
                ShellValueUnsetType::Untyped
            };

            let mut var = ShellVariable::new(ShellValue::Unset(unset_type));

            self.apply_attributes_before_update(&mut var)?;

            if let Some(initial_value) = initial_value {
                var.assign(initial_value, false)?;
            }

            self.apply_attributes_after_update(&mut var, verb)?;

            let scope = if create_var_local {
                EnvironmentScope::Local
            } else {
                EnvironmentScope::Global
            };

            context.shell.env.add(name, var, scope)?;
        }

        Ok(true)
    }

    #[allow(clippy::unwrap_in_result)]
    fn declaration_to_name_and_value(
        declaration: &commands::CommandArg,
    ) -> Result<(String, Option<String>, Option<ShellValueLiteral>, bool), error::Error> {
        let name;
        let assigned_index;
        let initial_value;
        let name_is_array;

        match declaration {
            commands::CommandArg::String(s) => {
                // We need to handle the case of someone invoking `declare array[index]`.
                // In such case, we ignore the index and treat it as a declaration of
                // the array.
                lazy_static::lazy_static! {
                    static ref ARRAY_AND_INDEX_RE: fancy_regex::Regex =
                        fancy_regex::Regex::new(r"^(.*?)\[(.*?)\]$").unwrap();
                }
                if let Some(captures) = ARRAY_AND_INDEX_RE.captures(s)? {
                    name = captures.get(1).unwrap().as_str().to_owned();
                    assigned_index = Some(captures.get(2).unwrap().as_str().to_owned());
                    name_is_array = true;
                } else {
                    name = s.clone();
                    assigned_index = None;
                    name_is_array = false;
                }
                initial_value = None;
            }
            commands::CommandArg::Assignment(assignment) => {
                match &assignment.name {
                    brush_parser::ast::AssignmentName::VariableName(var_name) => {
                        name = var_name.to_owned();
                        assigned_index = None;
                    }
                    brush_parser::ast::AssignmentName::ArrayElementName(var_name, index) => {
                        if matches!(
                            assignment.value,
                            brush_parser::ast::AssignmentValue::Array(_)
                        ) {
                            return Err(error::Error::AssigningListToArrayMember);
                        }

                        name = var_name.to_owned();
                        assigned_index = Some(index.to_owned());
                    }
                }

                match &assignment.value {
                    brush_parser::ast::AssignmentValue::Scalar(s) => {
                        if let Some(index) = &assigned_index {
                            initial_value = Some(ShellValueLiteral::Array(ArrayLiteral(vec![(
                                Some(index.to_owned()),
                                s.value.clone(),
                            )])));
                            name_is_array = true;
                        } else {
                            initial_value = Some(ShellValueLiteral::Scalar(s.value.clone()));
                            name_is_array = false;
                        }
                    }
                    brush_parser::ast::AssignmentValue::Array(a) => {
                        initial_value = Some(ShellValueLiteral::Array(ArrayLiteral(
                            a.iter()
                                .map(|(i, v)| {
                                    (i.as_ref().map(|w| w.value.clone()), v.value.clone())
                                })
                                .collect(),
                        )));
                        name_is_array = true;
                    }
                }
            }
        }

        Ok((name, assigned_index, initial_value, name_is_array))
    }

    fn display_matching_env_declarations(
        &self,
        context: &mut crate::commands::ExecutionContext<'_>,
        verb: DeclareVerb,
    ) -> Result<(), error::Error> {
        //
        // Dump all declarations. Use attribute flags to filter which variables are dumped.
        //

        // We start by excluding all variables that are not enumerable.
        #[allow(clippy::type_complexity)]
        let mut filters: Vec<Box<dyn Fn((&String, &ShellVariable)) -> bool>> =
            vec![Box::new(|(_, v)| v.is_enumerable())];

        // Add filters depending on verb.
        if matches!(verb, DeclareVerb::Readonly) {
            filters.push(Box::new(|(_, v)| v.is_readonly()));
        }

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
            filters.push(Box::new(move |(_, v)| v.is_treated_as_nameref() == value));
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

        let iter_policy = if matches!(verb, DeclareVerb::Local) {
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
                let mut cs = variable.get_attribute_flags();
                if cs.is_empty() {
                    cs.push('-');
                }

                let separator_str = if matches!(variable.value(), ShellValue::Unset(_)) {
                    ""
                } else {
                    "="
                };

                writeln!(
                    context.stdout(),
                    "declare -{cs} {name}{separator_str}{}",
                    variable
                        .value()
                        .format(variables::FormatStyle::DeclarePrint)?
                )?;
            } else {
                writeln!(
                    context.stdout(),
                    "{name}={}",
                    variable.value().format(variables::FormatStyle::Basic)?
                )?;
            }
        }

        Ok(())
    }

    fn display_matching_functions(
        &self,
        context: &mut crate::commands::ExecutionContext<'_>,
    ) -> Result<(), error::Error> {
        for (name, registration) in context.shell.funcs.iter().sorted_by_key(|v| v.0) {
            if self.function_names_only {
                writeln!(context.stdout(), "declare -f {name}")?;
            } else {
                writeln!(context.stdout(), "{}", registration.definition)?;
            }
        }

        Ok(())
    }

    #[allow(clippy::unnecessary_wraps)]
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
                var.treat_as_nameref();
            } else {
                var.unset_treat_as_nameref();
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
    fn apply_attributes_after_update(
        &self,
        var: &mut ShellVariable,
        verb: DeclareVerb,
    ) -> Result<(), error::Error> {
        if matches!(verb, DeclareVerb::Readonly) {
            var.set_readonly();
        } else if let Some(value) = self.make_readonly.to_bool() {
            if value {
                var.set_readonly();
            } else {
                var.unset_readonly()?;
            }
        }

        Ok(())
    }
}
