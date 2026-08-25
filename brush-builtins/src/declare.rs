use itertools::Itertools;
use std::{io::Write, sync::LazyLock};

use brush_core::argmodel::{ArgSpec, CommandSpec, ParsedValues};
use brush_core::{
    ErrorKind, ExecutionResult, builtins,
    env::{self, EnvironmentLookup, EnvironmentScope},
    error,
    parser::ast,
    variables::{
        self, ArrayLiteral, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
        ShellVariableUpdateTransform,
    },
};

const ID_FUNCTION_NAMES_OR_DEFS_ONLY: &str = "function_names_or_defs_only";
const ID_FUNCTION_NAMES_ONLY: &str = "function_names_only";
const ID_CREATE_GLOBAL: &str = "create_global";
const ID_LOCALS_INHERIT_FROM_PREV_SCOPE: &str = "locals_inherit_from_prev_scope";
const ID_PRINT: &str = "print";

/// Display or update variables and their attributes.
pub(crate) struct DeclareCommand {
    function_names_or_defs_only: bool,
    function_names_only: bool,
    create_global: bool,
    locals_inherit_from_prev_scope: bool,
    print: bool,

    // Attribute options
    make_indexed_array: Option<bool>,
    make_associative_array: Option<bool>,
    capitalize_value_on_assignment: Option<bool>,
    make_integer: Option<bool>,
    lowercase_value_on_assignment: Option<bool>,
    make_nameref: Option<bool>,
    make_readonly: Option<bool>,
    make_traced: Option<bool>,
    uppercase_value_on_assignment: Option<bool>,
    make_exported: Option<bool>,

    // N.B. These are skipped during parsing, but filled in by the
    // SpecCommand trait.
    declarations: Vec<brush_core::CommandArg>,
}

/// Expands groups of `+`-style options (e.g., `+ax`) into individual hidden
/// long spellings (e.g., `--+a --+x`) that the argument backend can match
/// against the disable-side arguments in this command's spec.
fn expand_plus_options(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .flat_map(|arg| {
            if let Some(group) = arg.strip_prefix('+').filter(|g| !g.is_empty()) {
                if group.starts_with('+') || group.contains('=') {
                    // Not an option group (e.g., `++x` or `+foo=bar`);
                    // pass it through unchanged.
                    vec![arg]
                } else {
                    group.chars().map(|c| format!("--+{c}")).collect::<Vec<_>>()
                }
            } else {
                vec![arg]
            }
        })
        .collect()
}

static DECLARE_SPEC: CommandSpec = CommandSpec {
    args: &[
        ArgSpec::flag(
            ID_FUNCTION_NAMES_OR_DEFS_ONLY,
            &['f'],
            &[],
            "Constrain to function names or definitions.",
        ),
        ArgSpec::flag(
            ID_FUNCTION_NAMES_ONLY,
            &['F'],
            &[],
            "Constrain to function names only.",
        ),
        ArgSpec::flag(
            ID_CREATE_GLOBAL,
            &['g'],
            &[],
            "Create global variable, if applicable.",
        ),
        ArgSpec::flag(
            ID_LOCALS_INHERIT_FROM_PREV_SCOPE,
            &['I'],
            &[],
            "When creating a local variable that shadows another variable of the same name, \
                     then initialize it with the contents and attributes of the variable being \
                     shadowed.",
        ),
        ArgSpec::flag(
            ID_PRINT,
            &['p'],
            &[],
            "Display each item's attributes and values.",
        ),
        ArgSpec::flag(
            "make_indexed_array_enable",
            &['a'],
            &[],
            "Make the variable an indexed array.",
        ),
        ArgSpec::hidden_flag("make_indexed_array_disable", &[], &["+a"], ""),
        ArgSpec::flag(
            "make_associative_array_enable",
            &['A'],
            &[],
            "Make the variable an associative array.",
        ),
        ArgSpec::hidden_flag("make_associative_array_disable", &[], &["+A"], ""),
        ArgSpec::flag(
            "capitalize_value_on_assignment_enable",
            &['c'],
            &[],
            "Enable capitalize-on-assignment for the variable.",
        ),
        ArgSpec::hidden_flag("capitalize_value_on_assignment_disable", &[], &["+c"], ""),
        ArgSpec::flag(
            "make_integer_enable",
            &['i'],
            &[],
            "Mark the variable as integer-typed",
        ),
        ArgSpec::hidden_flag("make_integer_disable", &[], &["+i"], ""),
        ArgSpec::flag(
            "lowercase_value_on_assignment_enable",
            &['l'],
            &[],
            "Enable lowercase-on-assignment for the variable.",
        ),
        ArgSpec::hidden_flag("lowercase_value_on_assignment_disable", &[], &["+l"], ""),
        ArgSpec::flag(
            "make_nameref_enable",
            &['n'],
            &[],
            "Mark the variable as a name reference",
        ),
        ArgSpec::hidden_flag("make_nameref_disable", &[], &["+n"], ""),
        ArgSpec::flag(
            "make_readonly_enable",
            &['r'],
            &[],
            "Mark the variable as read-only.",
        ),
        ArgSpec::hidden_flag("make_readonly_disable", &[], &["+r"], ""),
        ArgSpec::flag(
            "make_traced_enable",
            &['t'],
            &[],
            "Enable tracing for the variable.",
        ),
        ArgSpec::hidden_flag("make_traced_disable", &[], &["+t"], ""),
        ArgSpec::flag(
            "uppercase_value_on_assignment_enable",
            &['u'],
            &[],
            "Enable uppercase-on-assignment for the variable.",
        ),
        ArgSpec::hidden_flag("uppercase_value_on_assignment_disable", &[], &["+u"], ""),
        ArgSpec::flag(
            "make_exported_enable",
            &['x'],
            &[],
            "Mark the variable for export.",
        ),
        ArgSpec::hidden_flag("make_exported_disable", &[], &["+x"], ""),
    ],
    positionals: &[],
};

impl builtins::SpecCommand for DeclareCommand {
    type Error = brush_core::Error;

    fn takes_plus_options() -> bool {
        true
    }

    fn uses_declarations() -> bool {
        true
    }

    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }

    fn spec() -> &'static CommandSpec {
        &DECLARE_SPEC
    }

    /// Overrides the default [`builtins::SpecCommand::new`] flow so that
    /// `+`-style option spellings are rewritten into forms the argument
    /// backend can match; see [`expand_plus_options`].
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        let expanded = expand_plus_options(args);

        let mut values = builtins::argmodel::backend().parse(Self::spec(), "", &expanded)?;

        Self::from_matches(&mut values)
    }

    fn from_matches(values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            function_names_or_defs_only: values.flag(ID_FUNCTION_NAMES_OR_DEFS_ONLY),
            function_names_only: values.flag(ID_FUNCTION_NAMES_ONLY),
            create_global: values.flag(ID_CREATE_GLOBAL),
            locals_inherit_from_prev_scope: values.flag(ID_LOCALS_INHERIT_FROM_PREV_SCOPE),
            print: values.flag(ID_PRINT),
            make_indexed_array: crate::read_plus_minus(
                values,
                "make_indexed_array_enable",
                "make_indexed_array_disable",
            ),
            make_associative_array: crate::read_plus_minus(
                values,
                "make_associative_array_enable",
                "make_associative_array_disable",
            ),
            capitalize_value_on_assignment: crate::read_plus_minus(
                values,
                "capitalize_value_on_assignment_enable",
                "capitalize_value_on_assignment_disable",
            ),
            make_integer: crate::read_plus_minus(
                values,
                "make_integer_enable",
                "make_integer_disable",
            ),
            lowercase_value_on_assignment: crate::read_plus_minus(
                values,
                "lowercase_value_on_assignment_enable",
                "lowercase_value_on_assignment_disable",
            ),
            make_nameref: crate::read_plus_minus(
                values,
                "make_nameref_enable",
                "make_nameref_disable",
            ),
            make_readonly: crate::read_plus_minus(
                values,
                "make_readonly_enable",
                "make_readonly_disable",
            ),
            make_traced: crate::read_plus_minus(
                values,
                "make_traced_enable",
                "make_traced_disable",
            ),
            uppercase_value_on_assignment: crate::read_plus_minus(
                values,
                "uppercase_value_on_assignment_enable",
                "uppercase_value_on_assignment_disable",
            ),
            make_exported: crate::read_plus_minus(
                values,
                "make_exported_enable",
                "make_exported_disable",
            ),

            declarations: Vec::new(),
        })
    }

    fn about() -> &'static str {
        "Display or update variables and their attributes."
    }

    fn synopsis() -> &'static str {
        "[OPTIONS] [DECLARATIONS]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let verb = match context.command_name.as_str() {
            "local" => DeclareVerb::Local,
            "readonly" => DeclareVerb::Readonly,
            _ => DeclareVerb::Declare,
        };

        if matches!(verb, DeclareVerb::Local) && !context.shell.in_function() {
            writeln!(context.stderr(), "can only be used in a function")?;
            return Ok(ExecutionResult::general_error());
        }

        if self.locals_inherit_from_prev_scope {
            return error::unimp("declare -I");
        }

        let mut result = ExecutionResult::success();
        if !self.declarations.is_empty() {
            for declaration in &self.declarations {
                if self.print && !matches!(verb, DeclareVerb::Readonly) {
                    if !self.try_display_declaration(&context, declaration, verb)? {
                        result = ExecutionResult::general_error();
                    }
                } else {
                    if !self.process_declaration(&mut context, declaration, verb)? {
                        result = ExecutionResult::general_error();
                    }
                }
            }
        } else {
            // Display matching declarations from the variable environment.
            if !self.function_names_only && !self.function_names_or_defs_only {
                self.display_matching_env_declarations(&context, verb)?;
            }

            // Do the same for functions.
            if !matches!(verb, DeclareVerb::Local | DeclareVerb::Readonly)
                && (!self.print || self.function_names_only || self.function_names_or_defs_only)
            {
                self.display_matching_functions(&context)?;
            }
        }

        Ok(result)
    }
}

#[derive(Clone, Copy)]
enum DeclareVerb {
    Declare,
    Local,
    Readonly,
}

impl DeclareCommand {
    fn try_display_declaration(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, brush_core::Error> {
        let name = match declaration {
            brush_core::CommandArg::String(s) => s,
            brush_core::CommandArg::Assignment(_) => {
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
            if let Some(func_registration) = context.shell.funcs().get(name) {
                if self.function_names_only {
                    if self.print {
                        writeln!(context.stdout(), "declare -f {name}")?;
                    } else {
                        writeln!(context.stdout(), "{name}")?;
                    }
                } else {
                    writeln!(context.stdout(), "{}", func_registration.definition())?;
                }
                Ok(true)
            } else {
                // For some reason, bash does not print an error message in this case.
                Ok(false)
            }
        } else if let Some(variable) = context.shell.env().get_using_policy(name, lookup) {
            let mut cs = variable.attribute_flags(context.shell);
            if cs.is_empty() {
                cs.push('-');
            }

            let resolved_value = variable.resolve_value(context.shell);
            let separator_str = if matches!(resolved_value, ShellValue::Unset(_)) {
                ""
            } else {
                "="
            };

            writeln!(
                context.stdout(),
                "declare -{cs} {name}{separator_str}{}",
                resolved_value.format(variables::FormatStyle::DeclarePrint, context.shell)?
            )?;

            Ok(true)
        } else {
            writeln!(context.stderr(), "declare: {name}: not found")?;
            Ok(false)
        }
    }

    fn process_declaration(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, brush_core::Error> {
        let create_var_local = matches!(verb, DeclareVerb::Local)
            || (matches!(verb, DeclareVerb::Declare)
                && context.shell.in_function()
                && !self.create_global);

        if self.function_names_or_defs_only || self.function_names_only {
            return self.try_display_declaration(context, declaration, verb);
        }

        // Extract the variable name and the initial value being assigned (if any).
        let (name, assigned_index, initial_value, name_is_array) =
            Self::declaration_to_name_and_value(declaration)?;

        // Special-case: `local -`
        if name == "-" && matches!(verb, DeclareVerb::Local) {
            // TODO(local): `local -` allows shadowing the current `set` options (i.e., $-), with
            // subsequent updates getting discarded when the current local scope is popped.
            tracing::warn!("not yet implemented: local -");
            return Ok(true);
        }

        // Make sure it's a valid name.
        if !env::valid_variable_name(name.as_str()) {
            writeln!(
                context.stderr(),
                "{}: {name}: not a valid variable name",
                context.command_name
            )?;
            return Ok(false);
        }

        // Figure out where we should look.
        let lookup = if create_var_local {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        // Look up the variable.
        if let Some(var) = context
            .shell
            .env_mut()
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

            if context.shell.options().export_variables_on_modification && !var.value().is_array() {
                var.export();
            }

            self.apply_attributes_after_update(&mut var, verb)?;

            let scope = if create_var_local {
                EnvironmentScope::Local
            } else {
                EnvironmentScope::Global
            };

            context.shell.env_mut().add(name, var, scope)?;
        }

        Ok(true)
    }

    fn declaration_to_name_and_value(
        declaration: &brush_core::CommandArg,
    ) -> Result<(String, Option<String>, Option<ShellValueLiteral>, bool), brush_core::Error> {
        let name;
        let assigned_index;
        let initial_value;
        let name_is_array;

        match declaration {
            brush_core::CommandArg::String(s) => {
                // We need to handle the case of someone invoking `declare array[index]`.
                // In such case, we ignore the index and treat it as a declaration of
                // the array.
                #[allow(
                    clippy::unwrap_in_result,
                    clippy::unwrap_used,
                    reason = "regex is valid and should not fail"
                )]
                static ARRAY_AND_INDEX_RE: LazyLock<fancy_regex::Regex> =
                    LazyLock::new(|| fancy_regex::Regex::new(r"^(.*?)\[(.*?)\]$").unwrap());

                if let Some(captures) = ARRAY_AND_INDEX_RE.captures(s)? {
                    name = captures
                        .get(1)
                        .ok_or_else(|| {
                            brush_core::ErrorKind::InternalError("declaration parse error".into())
                        })?
                        .as_str()
                        .to_owned();

                    assigned_index = captures.get(2).map(|m| m.as_str().to_owned());
                    name_is_array = true;
                } else {
                    name = s.clone();
                    assigned_index = None;
                    name_is_array = false;
                }
                initial_value = None;
            }
            brush_core::CommandArg::Assignment(assignment) => {
                match &assignment.name {
                    ast::AssignmentName::VariableName(var_name) => {
                        name = var_name.to_owned();
                        assigned_index = None;
                    }
                    ast::AssignmentName::ArrayElementName(var_name, index) => {
                        if matches!(assignment.value, ast::AssignmentValue::Array(_)) {
                            return Err(ErrorKind::AssigningListToArrayMember.into());
                        }

                        name = var_name.to_owned();
                        assigned_index = Some(index.to_owned());
                    }
                }

                match &assignment.value {
                    ast::AssignmentValue::Scalar(s) => {
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
                    ast::AssignmentValue::Array(a) => {
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
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        verb: DeclareVerb,
    ) -> Result<(), brush_core::Error> {
        //
        // Dump all declarations. Use attribute flags to filter which variables are dumped.
        //

        // We start by excluding all variables that are not enumerable.
        #[expect(clippy::type_complexity)]
        let mut filters: Vec<Box<dyn Fn((&String, &ShellVariable)) -> bool>> =
            vec![Box::new(|(_, v)| v.is_enumerable())];

        // Add filters depending on verb.
        if matches!(verb, DeclareVerb::Readonly) {
            filters.push(Box::new(|(_, v)| v.is_readonly()));
        }

        // Add filters depending on attribute flags.
        if let Some(value) = self.make_indexed_array {
            filters.push(Box::new(move |(_, v)| {
                matches!(v.value(), ShellValue::IndexedArray(_)) == value
            }));
        }
        if let Some(value) = self.make_associative_array {
            filters.push(Box::new(move |(_, v)| {
                matches!(v.value(), ShellValue::AssociativeArray(_)) == value
            }));
        }
        if let Some(value) = self.make_integer {
            filters.push(Box::new(move |(_, v)| v.is_treated_as_integer() == value));
        }
        if let Some(value) = self.capitalize_value_on_assignment {
            filters.push(Box::new(move |(_, v)| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Capitalize
                ) == value
            }));
        }
        if let Some(value) = self.lowercase_value_on_assignment {
            filters.push(Box::new(move |(_, v)| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Lowercase
                ) == value
            }));
        }
        if let Some(value) = self.make_nameref {
            filters.push(Box::new(move |(_, v)| v.is_treated_as_nameref() == value));
        }
        if let Some(value) = self.make_readonly {
            filters.push(Box::new(move |(_, v)| v.is_readonly() == value));
        }
        if let Some(value) = self.make_readonly {
            filters.push(Box::new(move |(_, v)| v.is_trace_enabled() == value));
        }
        if let Some(value) = self.uppercase_value_on_assignment {
            filters.push(Box::new(move |(_, v)| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Uppercase
                ) == value
            }));
        }
        if let Some(value) = self.make_exported {
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
            .env()
            .iter_using_policy(iter_policy)
            .filter(|pair| filters.iter().all(|f| f(*pair)))
            .sorted_by_key(|v| v.0)
        {
            if self.print {
                let mut cs = variable.attribute_flags(context.shell);
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
                        .format(variables::FormatStyle::DeclarePrint, context.shell)?
                )?;
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

    fn display_matching_functions(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<(), brush_core::Error> {
        for (name, registration) in context.shell.funcs().iter().sorted_by_key(|v| v.0) {
            if self.function_names_only {
                writeln!(context.stdout(), "declare -f {name}")?;
            } else {
                writeln!(context.stdout(), "{}", registration.definition())?;
            }
        }

        Ok(())
    }

    #[expect(clippy::unnecessary_wraps)]
    const fn apply_attributes_before_update(
        &self,
        var: &mut ShellVariable,
    ) -> Result<(), brush_core::Error> {
        if let Some(value) = self.make_integer {
            if value {
                var.treat_as_integer();
            } else {
                var.unset_treat_as_integer();
            }
        }
        if let Some(value) = self.capitalize_value_on_assignment {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Capitalize);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Capitalize
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.lowercase_value_on_assignment {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Lowercase);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Lowercase
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.make_nameref {
            if value {
                var.treat_as_nameref();
            } else {
                var.unset_treat_as_nameref();
            }
        }
        if let Some(value) = self.make_traced {
            if value {
                var.enable_trace();
            } else {
                var.disable_trace();
            }
        }
        if let Some(value) = self.uppercase_value_on_assignment {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Uppercase);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Uppercase
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
            }
        }
        if let Some(value) = self.make_exported {
            if value {
                var.export();
            } else {
                var.unexport();
            }
        }

        Ok(())
    }

    fn apply_attributes_after_update(
        &self,
        var: &mut ShellVariable,
        verb: DeclareVerb,
    ) -> Result<(), brush_core::Error> {
        if matches!(verb, DeclareVerb::Readonly) {
            var.set_readonly();
        } else if let Some(value) = self.make_readonly {
            if value {
                var.set_readonly();
            } else {
                var.unset_readonly()?;
            }
        }

        Ok(())
    }
}
