use clap::Parser;
use itertools::Itertools;
use std::{io::Write, sync::LazyLock};

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

crate::minus_or_plus_flag_arg!(
    MakeIndexedArrayFlag,
    'a',
    "Make the variable an indexed array."
);
crate::minus_or_plus_flag_arg!(
    MakeAssociativeArrayFlag,
    'A',
    "Make the variable an associative array."
);
crate::minus_or_plus_flag_arg!(
    CapitalizeValueOnAssignmentFlag,
    'c',
    "Enable capitalize-on-assignment for the variable."
);
crate::minus_or_plus_flag_arg!(MakeIntegerFlag, 'i', "Mark the variable as integer-typed");
crate::minus_or_plus_flag_arg!(
    LowercaseValueOnAssignmentFlag,
    'l',
    "Enable lowercase-on-assignment for the variable."
);
crate::minus_or_plus_flag_arg!(
    MakeNameRefFlag,
    'n',
    "Mark the variable as a name reference"
);
crate::minus_or_plus_flag_arg!(MakeReadonlyFlag, 'r', "Mark the variable as read-only.");
crate::minus_or_plus_flag_arg!(MakeTracedFlag, 't', "Enable tracing for the variable.");
crate::minus_or_plus_flag_arg!(
    UppercaseValueOnAssignmentFlag,
    'u',
    "Enable uppercase-on-assignment for the variable."
);
crate::minus_or_plus_flag_arg!(MakeExportedFlag, 'x', "Mark the variable for export.");

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
    #[clap(flatten)] // -a
    make_indexed_array: MakeIndexedArrayFlag,
    #[clap(flatten)] // -A
    make_associative_array: MakeAssociativeArrayFlag,
    #[clap(flatten)] // -c
    capitalize_value_on_assignment: CapitalizeValueOnAssignmentFlag,
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
    #[clap(skip)]
    declarations: Vec<brush_core::CommandArg>,
}

#[derive(Clone, Copy)]
enum DeclareVerb {
    Declare,
    Local,
    Readonly,
}

impl builtins::DeclarationCommand for DeclareCommand {
    fn set_declarations(&mut self, declarations: Vec<brush_core::CommandArg>) {
        self.declarations = declarations;
    }
}

impl builtins::Command for DeclareCommand {
    fn takes_plus_options() -> bool {
        true
    }

    type Error = brush_core::Error;

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
        } else if let Some((_, variable)) = context
            .shell
            .env()
            .lookup(name.as_str())
            .bypassing_nameref()
            .in_scope(lookup)
            .get()
        {
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

    #[expect(clippy::too_many_lines)]
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

        // In bash, `declare -ni var=value` fails — the combination of nameref
        // and integer attributes with an initial value is rejected. Without a
        // value (e.g., `declare -ni var`), bash applies both attributes.
        let nameref_integer_conflict = matches!(self.make_nameref.to_bool(), Some(true))
            && matches!(self.make_integer.to_bool(), Some(true))
            && initial_value.is_some();
        if nameref_integer_conflict {
            return Ok(false);
        }

        // Figure out where we should look.
        let lookup = if create_var_local {
            EnvironmentLookup::OnlyInCurrentLocal
        } else {
            EnvironmentLookup::Anywhere
        };

        // Resolve the nameref chain ONCE upfront, if applicable. The result
        // drives both the readonly subscripted-target rejection and the
        // attribute-change resolution below.
        //
        // Skip resolution when -n is being explicitly set/unset (we operate
        // on the nameref itself) or when creating a new local that doesn't
        // shadow an existing one.
        let explicitly_modifying_nameref_attr = self.make_nameref.to_bool().is_some();
        let creating_new_local = create_var_local
            && context
                .shell
                .env()
                .lookup(name.as_str())
                .bypassing_nameref()
                .in_scope(lookup)
                .get()
                .is_none();
        // Cycle in the nameref chain: bash warns and treats as identity (the
        // attribute change applies to the nameref itself), exit 0.
        let resolved = if !explicitly_modifying_nameref_attr && !creating_new_local {
            match context.shell.env().resolve_nameref(name.as_str()) {
                Ok(r) => Some(r),
                Err(err) if matches!(err.kind(), error::ErrorKind::CircularNameReference(_)) => {
                    context.shell.warn_circular_nameref(&err)?;
                    None
                }
                Err(err) => return Err(err),
            }
        } else {
            None
        };

        // `readonly` rejects subscripted nameref targets; `declare -r` applies
        // the attribute to the base variable. This asymmetry matches bash.
        if matches!(verb, DeclareVerb::Readonly)
            && let Some(r) = resolved.as_ref()
            && let Some(idx) = r.subscript()
        {
            writeln!(
                context.stderr(),
                "{}: `{}[{idx}]': not a valid identifier",
                context.command_name,
                r.name(),
            )?;
            return Ok(false);
        }

        // Apply nameref resolution to the (name, lookup) pair for attribute
        // changes. Subscripted targets resolve to the base; non-namerefs and
        // identity resolutions fall through unchanged.
        let (name, lookup) = match resolved {
            Some(r) if r.name() != name.as_str() => (r.into_name(), EnvironmentLookup::Anywhere),
            _ => (name, lookup),
        };

        let will_be_nameref = self.will_be_nameref();
        if will_be_nameref
            && let Some(ShellValueLiteral::Scalar(target)) = initial_value.as_ref()
            && let Some(msg) = Self::validate_nameref_target(Some(name.as_str()), target)
        {
            writeln!(context.stderr(), "{}: {msg}", context.command_name)?;
            return Ok(false);
        }

        // Look up the variable. Name is already resolved through
        // resolve_nameref_for_declaration above.
        let resolved_name = env::ResolvedName::plain(name.as_str());
        if let Some((_, var)) = context
            .shell
            .env_mut()
            .lookup_mut_resolved(&resolved_name)
            .in_scope(lookup)
            .get()
        {
            // Bash rejects `declare -n` on an existing indexed/associative
            // array variable with "reference variable cannot be an array".
            if will_be_nameref
                && matches!(
                    var.value(),
                    ShellValue::IndexedArray(_)
                        | ShellValue::AssociativeArray(_)
                        | ShellValue::Unset(
                            ShellValueUnsetType::IndexedArray
                                | ShellValueUnsetType::AssociativeArray
                        )
                )
            {
                writeln!(
                    context.stderr(),
                    "{}: {name}: reference variable cannot be an array",
                    context.command_name,
                )?;
                return Ok(false);
            }

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

            // Validate existing value when -n is being added to a variable
            // that wasn't given a new value (e.g., `x=x; declare -n x`).
            // No self-ref check — bash allows implicit self-refs at this stage.
            if var.is_treated_as_nameref()
                && let ShellValue::String(target) = var.value()
                && let Some(msg) = Self::validate_nameref_target(None, target)
            {
                writeln!(context.stderr(), "{}: {msg}", context.command_name)?;
                return Ok(false);
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

            // Validate nameref target name after assignment.
            if var.is_treated_as_nameref()
                && let ShellValue::String(target) = var.value()
                && let Some(msg) = Self::validate_nameref_target(None, target)
            {
                writeln!(context.stderr(), "{}: {msg}", context.command_name)?;
                return Ok(false);
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

            // add() — not update_or_add — so `declare -n ref=target` defines
            // the nameref itself, doesn't write through an existing one.
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

    /// Validates a nameref target name. If `creation_var_name` is `Some`,
    /// rejects explicit self-references (`declare -n x=x`); if `None`, allows
    /// them — bash permits implicit self-refs like `x=x; declare -n x`.
    /// Empty targets are always accepted.
    fn validate_nameref_target(creation_var_name: Option<&str>, target: &str) -> Option<String> {
        if target.is_empty() {
            return None;
        }
        if let Some(var_name) = creation_var_name
            && target == var_name
        {
            return Some(format!(
                "{var_name}: nameref variable self references not allowed"
            ));
        }
        if !env::valid_nameref_target_name(target) {
            return Some(format!(
                "`{target}': invalid variable name for name reference"
            ));
        }
        None
    }

    /// Determines whether this declaration will effectively create a nameref,
    /// accounting for flag conflicts (`-na`, `-nA`) that suppress `-n`.
    ///
    /// The `-ni` combination does NOT suppress `-n` — bash applies both
    /// attributes when no initial value is provided. (With an initial value,
    /// the declaration is rejected early in `process_declaration`.)
    const fn will_be_nameref(&self) -> bool {
        matches!(self.make_nameref.to_bool(), Some(true))
            && !(self.make_indexed_array.is_some() || self.make_associative_array.is_some())
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
        if let Some(value) = self.capitalize_value_on_assignment.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                matches!(
                    v.get_update_transform(),
                    ShellVariableUpdateTransform::Capitalize
                ) == value
            }));
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
        // -na / -nA: -n is silently dropped in favor of the array flag.
        // -ni with an initial value is rejected earlier in process_declaration.
        let suppress_nameref = matches!(self.make_nameref.to_bool(), Some(true))
            && (self.make_indexed_array.is_some() || self.make_associative_array.is_some());

        if let Some(value) = self.make_integer.to_bool() {
            if value {
                var.treat_as_integer();
            } else {
                var.unset_treat_as_integer();
            }
        }
        if let Some(value) = self.capitalize_value_on_assignment.to_bool() {
            if value {
                var.set_update_transform(ShellVariableUpdateTransform::Capitalize);
            } else if matches!(
                var.get_update_transform(),
                ShellVariableUpdateTransform::Capitalize
            ) {
                var.set_update_transform(ShellVariableUpdateTransform::None);
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
        if !suppress_nameref {
            if let Some(value) = self.make_nameref.to_bool() {
                if value {
                    var.treat_as_nameref();
                } else {
                    var.unset_treat_as_nameref();
                }
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

    fn apply_attributes_after_update(
        &self,
        var: &mut ShellVariable,
        verb: DeclareVerb,
    ) -> Result<(), brush_core::Error> {
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
