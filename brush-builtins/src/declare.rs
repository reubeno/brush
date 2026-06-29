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

struct DeclarationParts {
    name: String,
    assigned_index: Option<String>,
    append: bool,
    initial_value: Option<ShellValueLiteral>,
    name_is_array: bool,
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
        let DeclarationParts {
            name,
            assigned_index,
            append,
            initial_value,
            name_is_array,
        } = Self::declaration_to_name_and_value(declaration)?;

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
        } else if self.create_global {
            // `declare -g` operates on the global scope, even when a same-name
            // local shadows it (bash: `local x; declare -g x=v` updates global).
            EnvironmentLookup::OnlyInGlobal
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
            let resolve_policy = if self.create_global {
                EnvironmentLookup::OnlyInGlobal
            } else {
                EnvironmentLookup::Anywhere
            };
            match context
                .shell
                .env()
                .resolve_nameref_using_policy(name.as_str(), resolve_policy)
            {
                Ok(r) => Some(r),
                Err(fault) => {
                    context.shell.warn_nameref_fault(&fault)?;
                    None
                }
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
        let (name, lookup, resolved_subscript) = match resolved {
            Some(r) if r.name() != name.as_str() || r.subscript().is_some() => {
                let subscript = r.subscript().map(str::to_owned);
                let lookup = if self.create_global {
                    lookup
                } else {
                    EnvironmentLookup::Anywhere
                };
                (r.into_name(), lookup, subscript)
            }
            _ => (name, lookup, None),
        };

        let will_be_nameref = self.will_be_nameref();
        if will_be_nameref
            && let Some(ShellValueLiteral::Scalar(target)) = initial_value.as_ref()
            && let Some(msg) =
                Self::validate_nameref_target(Some(name.as_str()), target, !create_var_local)
        {
            writeln!(context.stderr(), "{}: {msg}", context.command_name)?;
            return Ok(false);
        }

        // An integer-typed initializer is arithmetically evaluated, matching
        // bash (`declare -i x=2+3` is 5, not the literal "2+3"). The plain
        // `ShellVariable::assign` only parses an integer literal, so we must
        // evaluate here — the integer attribute applies when `-i` is being set,
        // or when modifying an existing integer var without `+i`.
        let will_be_integer = match self.make_integer.to_bool() {
            Some(set_integer) => set_integer,
            None => context
                .shell
                .env()
                .lookup_resolved(&env::ResolvedName::plain(name.as_str()))
                .in_scope(lookup)
                .get()
                .is_some_and(|(_, v)| v.is_treated_as_integer()),
        };
        let initial_value = match (will_be_integer, initial_value) {
            (true, Some(value)) => Some(Self::eval_integer_initializer(context, value)?),
            (_, other) => other,
        };

        // Indexed-array element subscripts in an initializer are arithmetic
        // expressions (`declare a=([i+1]=v)` → index 1); associative-array keys
        // are literal strings. Evaluate the keys for an indexed target.
        let target_is_associative = self.make_associative_array.to_bool() == Some(true)
            || (self.make_indexed_array.to_bool() != Some(true)
                && !name_is_array
                && context
                    .shell
                    .env()
                    .lookup_resolved(&env::ResolvedName::plain(name.as_str()))
                    .in_scope(lookup)
                    .get()
                    .is_some_and(|(_, v)| {
                        matches!(
                            v.value(),
                            ShellValue::AssociativeArray(_)
                                | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
                        )
                    }));
        let initial_value = match initial_value {
            Some(ShellValueLiteral::Array(array)) if !target_is_associative => Some(
                ShellValueLiteral::Array(Self::eval_indexed_array_keys(context, array)?),
            ),
            other => other,
        };

        // Look up the variable. Name is already resolved through
        // resolve_nameref_for_declaration above.
        let resolved_name = env::ResolvedName::plain(name.as_str());
        let resolved_subscript_index = if let Some(index) = resolved_subscript.as_deref() {
            Some(Self::resolved_subscript_for_assignment(
                context,
                &resolved_name,
                index,
            )?)
        } else {
            None
        };
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

            if will_be_nameref
                && initial_value.is_none()
                && !var.is_treated_as_nameref()
                && let ShellValue::String(target) = var.value()
                && let Some(msg) = Self::validate_nameref_target(None, target, !create_var_local)
            {
                writeln!(context.stderr(), "{}: {msg}", context.command_name)?;
                return Ok(false);
            }

            self.apply_attributes_before_update(var)?;

            if let Some(initial_value) = initial_value {
                if let Some(index) = resolved_subscript_index.as_ref()
                    && let ShellValueLiteral::Scalar(value) = initial_value
                {
                    var.assign_at_index(index.clone(), value, append)?;
                } else {
                    // We append if the declaration used += or included an explicit index.
                    var.assign(initial_value, append || assigned_index.is_some())?;
                }
            }

            // Validate existing value when -n is being added to a variable
            // that wasn't given a new value (e.g., `x=x; declare -n x`).
            // No self-ref check — bash allows implicit self-refs at this stage.
            if var.is_treated_as_nameref()
                && let ShellValue::String(target) = var.value()
                && let Some(msg) = Self::validate_nameref_target(None, target, !create_var_local)
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
                if let Some(index) = resolved_subscript_index
                    && let ShellValueLiteral::Scalar(value) = initial_value
                {
                    var.assign(
                        ShellValueLiteral::Array(ArrayLiteral(vec![(Some(index), value)])),
                        false,
                    )?;
                } else {
                    var.assign(initial_value, false)?;
                }
            }

            // Validate nameref target name after assignment.
            if var.is_treated_as_nameref()
                && let ShellValue::String(target) = var.value()
                && let Some(msg) = Self::validate_nameref_target(None, target, !create_var_local)
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
    ) -> Result<DeclarationParts, brush_core::Error> {
        let name;
        let assigned_index;
        let append;
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

                // A declaration argument formed by expansion (e.g.
                // `declare ${pre}var=val` or `declare "$assignment"`) reaches us
                // as an already-expanded string instead of a parsed assignment.
                // bash re-parses declaration-builtin arguments as assignments,
                // so split a leading `name`/`name[idx]` from a `=value` suffix.
                let (lhs, value_str) = Self::split_declaration_word(s);

                if let Some(captures) = ARRAY_AND_INDEX_RE.captures(lhs)? {
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
                    name = lhs.to_owned();
                    assigned_index = None;
                    name_is_array = false;
                }
                append = false;

                // A `name[idx]=value` element becomes a single-element array
                // literal (mirroring the parsed-assignment path); `name=value` a
                // scalar; a bare `name`/`name[idx]` carries no initial value.
                initial_value = match (value_str, &assigned_index) {
                    (Some(value), Some(index)) => Some(ShellValueLiteral::Array(ArrayLiteral(
                        vec![(Some(index.clone()), value.to_owned())],
                    ))),
                    (Some(value), None) => Some(ShellValueLiteral::Scalar(value.to_owned())),
                    (None, _) => None,
                };
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
                append = assignment.append;

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

        Ok(DeclarationParts {
            name,
            assigned_index,
            append,
            initial_value,
            name_is_array,
        })
    }

    fn resolved_subscript_for_assignment(
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        resolved_name: &env::ResolvedName,
        index: &str,
    ) -> Result<String, brush_core::Error> {
        let is_associative = context
            .shell
            .env()
            .lookup_resolved(resolved_name)
            .in_scope(
                resolved_name
                    .resolved_scope()
                    .lookup_policy_or(EnvironmentLookup::Anywhere),
            )
            .get()
            .is_some_and(|(_, v)| {
                matches!(
                    v.value(),
                    ShellValue::AssociativeArray(_)
                        | ShellValue::Unset(ShellValueUnsetType::AssociativeArray)
                )
            });
        if is_associative {
            Ok(index.to_owned())
        } else {
            Self::eval_arith_to_string(context, index)
        }
    }

    /// Splits an expansion-formed declaration word into its `name`/`name[idx]`
    /// part and an optional `=value` suffix. The split is on the first top-level
    /// `=` (one not nested inside `[...]`), so an arithmetic subscript like
    /// `arr[i=1]` keeps its `=`. Returns `(lhs, None)` when there is no `=`.
    fn split_declaration_word(s: &str) -> (&str, Option<&str>) {
        let mut bracket_depth = 0usize;
        // `[`, `]`, and `=` are all ASCII, so a byte index at one of them is
        // always a valid char boundary; `get` keeps this panic-free regardless.
        for (i, b) in s.bytes().enumerate() {
            match b {
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'=' if bracket_depth == 0 => {
                    return (s.get(..i).unwrap_or(s), s.get(i + 1..));
                }
                _ => {}
            }
        }
        (s, None)
    }

    /// Arithmetically evaluates an integer variable's initializer (`declare -i
    /// x=2+3` stores 5, not the literal `"2+3"`). Scalars are evaluated
    /// directly; each element of an `-ia`/`-iA` array initializer is evaluated
    /// independently (`declare -ia a=(1+1 2+2)` → `2 4`).
    fn eval_integer_initializer(
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        value: ShellValueLiteral,
    ) -> Result<ShellValueLiteral, brush_core::Error> {
        Ok(match value {
            ShellValueLiteral::Scalar(s) => {
                ShellValueLiteral::Scalar(Self::eval_arith_to_string(context, &s)?)
            }
            ShellValueLiteral::Array(ArrayLiteral(elements)) => {
                let mut evaluated = Vec::with_capacity(elements.len());
                for (key, element) in elements {
                    evaluated.push((key, Self::eval_arith_to_string(context, &element)?));
                }
                ShellValueLiteral::Array(ArrayLiteral(evaluated))
            }
        })
    }

    /// Arithmetically evaluates the explicit subscripts of an indexed-array
    /// initializer (`declare a=([i+1]=v)` → index 1), leaving element values and
    /// keyless entries untouched.
    fn eval_indexed_array_keys(
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        array: ArrayLiteral,
    ) -> Result<ArrayLiteral, brush_core::Error> {
        let mut evaluated = Vec::with_capacity(array.0.len());
        for (key, value) in array.0 {
            let key = match key {
                Some(key) => Some(Self::eval_arith_to_string(context, &key)?),
                None => None,
            };
            evaluated.push((key, value));
        }
        Ok(ArrayLiteral(evaluated))
    }

    /// Parses and evaluates an arithmetic expression, returning its decimal
    /// string form. The input has already been word-expanded by the time a
    /// declaration's value reaches here, so no further expansion is needed.
    fn eval_arith_to_string(
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        expr: &str,
    ) -> Result<String, brush_core::Error> {
        let parsed = brush_parser::arithmetic::parse(expr)?;
        Ok(context.shell.eval_arithmetic(&parsed)?.to_string())
    }

    /// Validates a nameref target name, returning an error message if invalid.
    /// If `creation_var_name` is `Some`, rejects explicit self-references
    /// (`declare -n x=x`) **only when `creating_at_global`** — bash allows a
    /// self-referential `local -n x=x` at function scope (it resolves to the
    /// global `x`), and permits implicit self-refs like `x=x; declare -n x`
    /// (`creation_var_name` is `None` there).
    ///
    /// This is only ever called with an *explicit* value, so an empty target
    /// means `declare -n ref=` / `declare -n ref=""`, which bash rejects as not
    /// a valid identifier. (A bare `declare -n ref` with no value never reaches
    /// here — it has no initial value to validate.)
    fn validate_nameref_target(
        creation_var_name: Option<&str>,
        target: &str,
        creating_at_global: bool,
    ) -> Option<String> {
        if target.is_empty() {
            return Some("`': not a valid identifier".to_owned());
        }
        if creating_at_global
            && let Some(var_name) = creation_var_name
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
