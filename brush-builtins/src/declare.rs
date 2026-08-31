use clap::Parser;
use itertools::Itertools;
use std::{borrow::Cow, io::Write, sync::LazyLock};

use brush_core::{
    ErrorKind, ExecutionResult, builtins,
    env::{self, EnvironmentLookup, EnvironmentScope},
    error,
    parser::ast,
    variables::{
        self, ArrayKind, ShellValue, ShellValueLiteral, ShellValueUnsetType, ShellVariable,
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

#[derive(Clone, Copy)]
struct DeclarationScope {
    lookup: EnvironmentLookup,
    creation: EnvironmentScope,
}

/// A declaration whose expansion and structural interpretation are complete.
struct PreparedDeclaration {
    /// The variable being declared.
    name: String,
    /// The subscript the operand named, if any. Present whenever the operand subscripted its
    /// target, even when it assigns nothing (as in `declare arr[5]`).
    subscript: Option<String>,
    /// The value to assign, if any. A compound value never accompanies a subscript.
    initial_value: Option<ShellValueLiteral>,
    /// Whether the operand appended rather than replaced.
    append: bool,
}

impl PreparedDeclaration {
    /// Converts an expanded, subscript-resolved assignment into a ready-to-apply declaration.
    /// Returns an error when a compound value targets a single array element.
    fn from_assignment(assignment: &ast::Assignment) -> Result<Self, error::Error> {
        let (name, subscript) = match &assignment.name {
            ast::AssignmentName::VariableName(name) => (name.to_owned(), None),
            ast::AssignmentName::ArrayElementName(name, index) => {
                if matches!(assignment.value, ast::AssignmentValue::Array(_)) {
                    return Err(ErrorKind::AssigningListToArrayMember.into());
                }

                (name.to_owned(), Some(index.to_owned()))
            }
        };

        Ok(Self {
            name,
            subscript,
            initial_value: Some(ShellValueLiteral::from(&assignment.value)),
            append: assignment.append,
        })
    }

    /// Returns whether this declaration implies an array-typed variable, either by subscripting
    /// its target or by supplying a compound value.
    const fn implies_array(&self) -> bool {
        self.subscript.is_some() || matches!(self.initial_value, Some(ShellValueLiteral::Array(_)))
    }

    /// Returns the text `readonly` echoes as an extra `set -x` trace line for this declaration, or
    /// `None` if this declaration is not echoed.
    ///
    /// Only a scalar assignment to a whole, validly named variable is echoed: a bare name assigns
    /// nothing, and a compound assignment is traced by a shell in a different form that is not
    /// reproduced here.
    fn render_traced_assignment(&self) -> Option<String> {
        let Some(value @ ShellValueLiteral::Scalar(_)) = &self.initial_value else {
            return None;
        };
        if self.subscript.is_some() || !env::valid_variable_name(self.name.as_str()) {
            return None;
        }

        let op = if self.append { "+=" } else { "=" };
        Some(std::format!("{}{op}{value}", self.name))
    }
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
            writeln!(
                context.stderr(),
                "{}: can only be used in a function",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }

        let mut result = ExecutionResult::success();
        if !self.declarations.is_empty() {
            if (self.print && !matches!(verb, DeclareVerb::Readonly))
                || self.function_names_only
                || self.function_names_or_defs_only
            {
                for declaration in &self.declarations {
                    if !self.try_display_declaration(&context, declaration, verb)? {
                        result = ExecutionResult::general_error();
                    }
                }
            } else {
                let scope = self.declaration_scope(&context, verb);

                // Every operand is interpreted against the environment as it existed before the
                // command, so prepare the complete batch before applying any of it.
                let mut prepared_declarations = Vec::with_capacity(self.declarations.len());
                for declaration in &self.declarations {
                    prepared_declarations.push(
                        self.prepare_declaration(&mut context, declaration, scope.lookup)
                            .await?,
                    );
                }

                // `readonly` echoes each of its assignments as an extra trace line, on top of
                // the trace the interpreter already emitted for the invocation itself.
                if matches!(verb, DeclareVerb::Readonly) {
                    context
                        .trace_extra_lines(
                            prepared_declarations
                                .iter()
                                .filter_map(PreparedDeclaration::render_traced_assignment),
                        )
                        .await;
                }

                for declaration in prepared_declarations {
                    if !self.apply_declaration(&mut context, declaration, verb, scope)? {
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
    /// Resolves and returns the lookup and creation scopes for this declaration invocation.
    fn declaration_scope(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        verb: DeclareVerb,
    ) -> DeclarationScope {
        let create_local = matches!(verb, DeclareVerb::Local)
            || (matches!(verb, DeclareVerb::Declare)
                && context.shell.in_function()
                && !self.create_global);

        let lookup = if create_local {
            EnvironmentLookup::OnlyInCurrentLocal
        } else if self.create_global {
            EnvironmentLookup::OnlyInGlobal
        } else {
            EnvironmentLookup::Anywhere
        };

        let creation = if create_local {
            EnvironmentScope::Local
        } else {
            EnvironmentScope::Global
        };

        DeclarationScope { lookup, creation }
    }

    /// Displays the variable or function named by a declaration argument. Returns `true` if the
    /// requested declaration was found and displayed.
    fn try_display_declaration(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, brush_core::Error> {
        let name = match declaration {
            brush_core::CommandArg::String(s) => s,
            brush_core::CommandArg::Assignment(assignment) => {
                // A display request cannot name an assignment; report the operand as given.
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
                        writeln!(context.stdout(), "declare -f {name}")?;
                    } else {
                        writeln!(context.stdout(), "{name}")?;
                    }
                } else {
                    writeln!(context.stdout(), "{}", func_registration.definition())?;
                }
                Ok(true)
            } else {
                // `declare -f`/`-F` and `typeset -f` report a missing function only through their
                // exit status; a shell prints nothing. `readonly -f` does print, but only because
                // it is not a display request at all -- see the known-failure case covering it.
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
            // Diagnostics name the builtin as invoked (`local`, `typeset`, ...), even though the
            // displayed declarations themselves always read `declare`.
            writeln!(
                context.stderr(),
                "{}: {name}: not found",
                context.command_name
            )?;
            Ok(false)
        }
    }

    /// Applies one prepared declaration to the variable environment. Returns `true` on success,
    /// or `false` for a declaration-level failure that should affect the command's exit status
    /// without aborting processing of subsequent declarations.
    fn apply_declaration(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: PreparedDeclaration,
        verb: DeclareVerb,
        scope: DeclarationScope,
    ) -> Result<bool, brush_core::Error> {
        // `+a` and `+A` cannot remove an array attribute from a variable that already has one;
        // that includes an array that was declared but never assigned. Report a declaration-level
        // failure rather than an internal error, so sibling operands still get processed.
        //
        // The environment is only consulted when one of those options is actually present, so the
        // ordinary path reaches the update below with a single lookup.
        let dropping_indexed = self.make_indexed_array.to_bool() == Some(false);
        let dropping_associative = self.make_associative_array.to_bool() == Some(false);
        if (dropping_indexed || dropping_associative)
            && let Some(existing) = context
                .shell
                .existing_array_kind(declaration.name.as_str(), scope.lookup)
            && match existing {
                ArrayKind::Indexed => dropping_indexed,
                ArrayKind::Associative => dropping_associative,
            }
        {
            writeln!(
                context.stderr(),
                "{}: {}: cannot destroy array variables in this way",
                context.command_name,
                declaration.name,
            )?;
            return Ok(false);
        }

        // Special-case: `local -`
        if declaration.name == "-" && matches!(verb, DeclareVerb::Local) {
            // TODO(local): `local -` allows shadowing the current `set` options (i.e., $-), with
            // subsequent updates getting discarded when the current local scope is popped.
            tracing::warn!("not yet implemented: local -");
            return Ok(true);
        }

        // Make sure it's a valid name.
        if !env::valid_variable_name(declaration.name.as_str()) {
            writeln!(
                context.stderr(),
                "{}: `{}': not a valid identifier",
                context.command_name,
                declaration.name,
            )?;
            return Ok(false);
        }

        // `local -I x[=v]` / `declare -I` (bash 5.0+) starts the new local from the nearest
        // same-name variable in an enclosing scope. With no such variable, fall through to
        // ordinary creation below.
        if let Some(inherited) = self.inherited_local(context, &declaration, scope) {
            return self.declare_inherited_local(context, declaration, verb, inherited);
        }

        // Look up the variable.
        if let Some(var) = context
            .shell
            .env_mut()
            .get_mut_using_policy(declaration.name.as_str(), scope.lookup)
        {
            match self.requested_array_kind() {
                Some(ArrayKind::Associative) => var.convert_to_associative_array()?,
                Some(ArrayKind::Indexed) => var.convert_to_indexed_array()?,
                None => (),
            }

            self.apply_attributes_before_update(var)?;

            if let Some(initial_value) = declaration.initial_value {
                assign_declaration_value(
                    var,
                    initial_value,
                    declaration.subscript.as_deref(),
                    declaration.append,
                )?;
            }

            self.apply_attributes_after_update(var, verb)?;
        } else {
            let unset_type = match self.requested_array_kind() {
                Some(ArrayKind::Indexed) => ShellValueUnsetType::IndexedArray,
                Some(ArrayKind::Associative) => ShellValueUnsetType::AssociativeArray,
                None if declaration.implies_array() => ShellValueUnsetType::IndexedArray,
                None => ShellValueUnsetType::Untyped,
            };

            let mut var = ShellVariable::new(ShellValue::Unset(unset_type));

            self.apply_attributes_before_update(&mut var)?;

            if let Some(initial_value) = declaration.initial_value {
                assign_declaration_value(
                    &mut var,
                    initial_value,
                    declaration.subscript.as_deref(),
                    declaration.append,
                )?;
            }

            if context.shell.options().export_variables_on_modification && !var.value().is_array() {
                var.export();
            }

            self.apply_attributes_after_update(&mut var, verb)?;

            context
                .shell
                .env_mut()
                .add(declaration.name, var, scope.creation)?;
        }

        Ok(true)
    }

    /// Returns a copy of the variable a `-I` declaration inherits, or `None` when this
    /// invocation did not ask to inherit, is not creating a local, or no same-name variable
    /// exists in an enclosing scope.
    fn inherited_local(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &PreparedDeclaration,
        scope: DeclarationScope,
    ) -> Option<ShellVariable> {
        if !self.locals_inherit_from_prev_scope || scope.creation != EnvironmentScope::Local {
            return None;
        }

        context
            .shell
            .env()
            .get_using_policy(declaration.name.as_str(), EnvironmentLookup::Anywhere)
            .cloned()
    }

    /// Applies a declaration onto a variable inherited from an enclosing scope, adding the result
    /// as a new local. Returns `true`, since an inherited declaration always succeeds once its
    /// source variable has been found.
    ///
    /// The inherited value is updated rather than replaced, so the assignment runs through the
    /// same helper as every other declared value; that is what makes `local -I name+=value`
    /// append to what was inherited.
    fn declare_inherited_local(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: PreparedDeclaration,
        verb: DeclareVerb,
        mut var: ShellVariable,
    ) -> Result<bool, brush_core::Error> {
        self.apply_attributes_before_update(&mut var)?;

        if let Some(initial_value) = declaration.initial_value {
            assign_declaration_value(
                &mut var,
                initial_value,
                declaration.subscript.as_deref(),
                declaration.append,
            )?;
        }

        if context.shell.options().export_variables_on_modification && !var.value().is_array() {
            var.export();
        }

        self.apply_attributes_after_update(&mut var, verb)?;

        context
            .shell
            .env_mut()
            .add(declaration.name, var, EnvironmentScope::Local)?;

        Ok(true)
    }

    /// Prepares one command argument for application. Returns the prepared declaration, or an
    /// error if its structured assignment is invalid.
    ///
    /// A [`brush_core::CommandArg::Assignment`] is an assignment the parser recognized in the
    /// command line; the interpreter has already expanded its words. A
    /// [`brush_core::CommandArg::String`] is any other operand: it may still turn out to hold
    /// assignment syntax (from quoting, or produced by an expansion), in which case that syntax is
    /// recognized here and its value is deliberately *not* expanded a second time.
    async fn prepare_declaration(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
        lookup: EnvironmentLookup,
    ) -> Result<PreparedDeclaration, brush_core::Error> {
        let assignment = match declaration {
            brush_core::CommandArg::Assignment(assignment) => Cow::Borrowed(assignment),
            brush_core::CommandArg::String(operand) => {
                match parse_string_operand(operand, &context.shell.parser_options())? {
                    StringOperand::Assignment(assignment) => Cow::Owned(assignment),
                    StringOperand::NameOnly(prepared) => return Ok(prepared),
                }
            }
        };

        // One lookup answers both questions the rest of preparation asks of the environment:
        // which subscript rules apply, and whether the target is already an array.
        let existing = context
            .shell
            .existing_array_kind(assignment.name.base_name(), lookup);
        let target = self.effective_array_kind(existing);

        let assignment = context
            .shell
            .resolve_assignment_subscripts(&context.params, &assignment, target)
            .await?;

        // A value that only now looks like compound syntax has to be reinterpreted before it can
        // be applied.
        match self
            .reinterpret_as_compound(context, &assignment, existing.is_some(), target)
            .await?
        {
            Some(compound) => PreparedDeclaration::from_assignment(&compound),
            None => PreparedDeclaration::from_assignment(&assignment),
        }
    }

    /// Returns the array kind this invocation explicitly requested with `-a` or `-A`, if any.
    ///
    /// A shell rejects those two options together; brush accepts them and lets `-A` win. See the
    /// `-a -A` known-failure case.
    fn requested_array_kind(&self) -> Option<ArrayKind> {
        if self.make_associative_array.to_bool() == Some(true) {
            Some(ArrayKind::Associative)
        } else if self.make_indexed_array.to_bool() == Some(true) {
            Some(ArrayKind::Indexed)
        } else {
            None
        }
    }

    /// Returns the array kind governing subscript expansion for this invocation. An explicit
    /// `-a`/`-A` wins; otherwise the target keeps whatever kind it already has.
    fn effective_array_kind(&self, existing: Option<ArrayKind>) -> ArrayKind {
        self.requested_array_kind()
            .or(existing)
            .unwrap_or(ArrayKind::Indexed)
    }

    /// Reinterprets an expanded assignment's scalar value as a compound array value, when the
    /// requested attributes or the target variable's existing type call for it. Returns the
    /// reinterpreted assignment, or `None` if the value should stay scalar.
    async fn reinterpret_as_compound(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        assignment: &ast::Assignment,
        target_is_array: bool,
        target: ArrayKind,
    ) -> Result<Option<ast::Assignment>, brush_core::Error> {
        // Absent an array attribute or an already-array target, text that looks like a compound
        // value stays scalar.
        if self.requested_array_kind().is_none() && !target_is_array {
            return Ok(None);
        }

        // Only a scalar value assigned to a whole variable is a candidate. Parser-recognized
        // compound assignments already had their elements expanded on the way in, and a compound
        // value cannot target a single array element. (A shell instead drops the subscript there;
        // see the `n[0]=(...)` known-failure case.)
        let ast::AssignmentValue::Scalar(value) = &assignment.value else {
            return Ok(None);
        };
        if !matches!(assignment.name, ast::AssignmentName::VariableName(_)) {
            return Ok(None);
        }

        let Some(elements) = brush_parser::word::parse_compound_assignment_value(
            value.value.as_str(),
            &context.shell.parser_options(),
        ) else {
            return Ok(None);
        };

        // The operand was already expanded once, but that expansion could not see words hidden
        // inside the compound syntax. Now that parsing has proven this is a compound value, expand
        // those newly recognized elements exactly once. Scalar declarations never reach here and
        // are therefore never re-expanded.
        let compound = ast::Assignment {
            name: assignment.name.clone(),
            value: ast::AssignmentValue::Array(elements),
            append: assignment.append,
            loc: assignment.loc.clone(),
        };

        Ok(Some(
            context
                .shell
                .expand_assignment(&context.params, &compound, target)
                .await?,
        ))
    }

    /// Displays all variables whose attributes match the requested filters.
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
        // N.B. An array that was declared but never assigned still carries the attribute, so these
        // use the same predicates the rest of the builtin does rather than matching on a populated
        // value.
        if let Some(value) = self.make_indexed_array.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                v.value().is_indexed_array() == value
            }));
        }
        if let Some(value) = self.make_associative_array.to_bool() {
            filters.push(Box::new(move |(_, v)| {
                v.value().is_associative_array() == value
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
        if let Some(value) = self.make_traced.to_bool() {
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

    /// Displays shell functions in the form requested by the command options.
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

    /// Applies attributes that must affect how a new value is assigned.
    #[expect(clippy::unnecessary_wraps)]
    const fn apply_attributes_before_update(
        &self,
        var: &mut ShellVariable,
    ) -> Result<(), brush_core::Error> {
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

    /// Applies readonly attributes after any value update has completed. Errors if readonly
    /// status cannot be removed.
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

/// The result of structurally interpreting a plain string operand.
enum StringOperand {
    /// The operand holds assignment syntax whose subscript still needs resolving.
    Assignment(ast::Assignment),
    /// The operand only names a variable, so no further interpretation is needed.
    NameOnly(PreparedDeclaration),
}

/// Interprets a plain string operand, returning either the assignment syntax it holds or a
/// ready-to-apply bare-name declaration.
///
/// The operand has already been through ordinary word expansion, so any assignment recognized here
/// keeps its value verbatim; only a subscript remains to be resolved by the caller.
fn parse_string_operand(
    operand: &str,
    parser_options: &brush_parser::ParserOptions,
) -> Result<StringOperand, error::Error> {
    // `declare array[index]` names an array without assigning to it. The subscript is retained
    // only to mark this as an array declaration; the element itself is never updated, so the
    // subscript is deliberately left unexpanded.
    #[expect(
        clippy::unwrap_used,
        reason = "regex is a compile-time constant and is known to be valid"
    )]
    static ARRAY_AND_INDEX_RE: LazyLock<fancy_regex::Regex> =
        LazyLock::new(|| fancy_regex::Regex::new(r"^(.*?)\[(.*?)\]$").unwrap());

    // Assignment syntax wins: it is the only interpretation under which the operand's text after
    // `=` is a value. Checking it first also keeps a value that merely ends in `]` (say,
    // `x=[a]`) from being mistaken for a subscripted name.
    if let Ok(assignment) = brush_parser::word::parse_scalar_assignment(operand, parser_options) {
        return Ok(StringOperand::Assignment(assignment));
    }

    if let Some(captures) = ARRAY_AND_INDEX_RE.captures(operand)?
        && let Some(name) = captures.get(1)
    {
        return Ok(StringOperand::NameOnly(PreparedDeclaration {
            name: name.as_str().to_owned(),
            subscript: captures.get(2).map(|m| m.as_str().to_owned()),
            initial_value: None,
            append: false,
        }));
    }

    // Just a name, as in `declare name`.
    Ok(StringOperand::NameOnly(PreparedDeclaration {
        name: operand.to_owned(),
        subscript: None,
        initial_value: None,
        append: false,
    }))
}

/// Assigns a declaration's value to a variable, targeting one array element when the declaration
/// carried an (already-resolved) subscript.
fn assign_declaration_value(
    variable: &mut ShellVariable,
    value: ShellValueLiteral,
    subscript: Option<&str>,
    append: bool,
) -> Result<(), error::Error> {
    match (value, subscript) {
        (ShellValueLiteral::Scalar(value), Some(index)) => {
            variable.assign_at_index(index.to_owned(), value, append)
        }
        // A compound value never reaches here alongside a subscript:
        // `PreparedDeclaration::from_assignment` rejects that combination before one is built.
        (value, _) => variable.assign(value, append),
    }
}
