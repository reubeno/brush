mod display;

use clap::Parser;
use std::borrow::Cow;
use std::io::Write;

use brush_core::{
    ErrorKind, ExecutionResult, builtins,
    env::{self, EnvironmentLookup, EnvironmentScope},
    expansion::ResolvedAssignment,
    parser::ast,
    variables::{
        self, ArrayKind, ScalarConversionPolicy, ShellValue, ShellValueLiteral,
        ShellValueUnsetType, ShellVariable, ShellVariableUpdateTransform,
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
    InheritLocalsFlag,
    'I',
    "Initialize a new local from the contents and attributes of the variable it shadows."
);
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
///
/// `export` and `readonly` are this command with a verb-implied attribute and a subset of its
/// options; they build one of these from their own command line and run it with
/// [`DeclareCommand::execute_as`].
#[derive(Parser, Default)]
#[clap(override_usage = "declare [OPTIONS] [DECLARATIONS]...")]
pub(crate) struct DeclareCommand {
    /// Constrain to function names or definitions.
    #[arg(short = 'f')]
    pub(crate) function_names_or_defs_only: bool,

    /// Constrain to function names only.
    #[arg(short = 'F')]
    pub(crate) function_names_only: bool,

    /// Create global variable, if applicable.
    #[arg(short = 'g')]
    pub(crate) create_global: bool,

    /// When creating a local variable that shadows another variable of the same name,
    /// then initialize it with the contents and attributes of the variable being shadowed.
    /// As in a shell, `+I` behaves like `-I`.
    #[clap(flatten)]
    pub(crate) locals_inherit_from_prev_scope: InheritLocalsFlag,

    /// Display each item's attributes and values.
    #[arg(short = 'p')]
    pub(crate) print: bool,

    //
    // Attribute options
    #[clap(flatten)] // -a
    pub(crate) make_indexed_array: MakeIndexedArrayFlag,
    #[clap(flatten)] // -A
    pub(crate) make_associative_array: MakeAssociativeArrayFlag,
    #[clap(flatten)] // -c
    pub(crate) capitalize_value_on_assignment: CapitalizeValueOnAssignmentFlag,
    #[clap(flatten)] // -i
    pub(crate) make_integer: MakeIntegerFlag,
    #[clap(flatten)] // -l
    pub(crate) lowercase_value_on_assignment: LowercaseValueOnAssignmentFlag,
    #[clap(flatten)] // -n
    pub(crate) make_nameref: MakeNameRefFlag,
    #[clap(flatten)] // -r
    pub(crate) make_readonly: MakeReadonlyFlag,
    #[clap(flatten)] // -t
    pub(crate) make_traced: MakeTracedFlag,
    #[clap(flatten)] // -u
    pub(crate) uppercase_value_on_assignment: UppercaseValueOnAssignmentFlag,
    #[clap(flatten)] // -x
    pub(crate) make_exported: MakeExportedFlag,

    //
    // Declarations
    //
    // N.B. These are skipped by clap, but filled in by the BuiltinDeclarationCommand trait.
    #[clap(skip)]
    pub(crate) declarations: Vec<brush_core::CommandArg>,
}

/// The builtin a declaration was invoked as. All of them share one implementation; the verb
/// selects the scope rules and whether an attribute is implied by the name -- which is where
/// nearly every behavioral difference between them comes from. See
/// [`DeclareVerb::implies_attribute`].
#[derive(Clone, Copy)]
pub(crate) enum DeclareVerb {
    Declare,
    // `local` and `readonly` are the only builtins declaring with these verbs, so the variants go
    // away with them.
    #[cfg_attr(
        not(feature = "builtin.declare"),
        allow(dead_code, reason = "constructed only by the `local` builtin")
    )]
    Local,
    #[cfg_attr(
        not(feature = "builtin.declare"),
        allow(dead_code, reason = "constructed only by the `readonly` builtin")
    )]
    Readonly,
    // `export` is the only builtin that declares with this verb, so the variant goes away with it.
    #[cfg_attr(
        not(feature = "builtin.export"),
        allow(dead_code, reason = "constructed only by the `export` builtin")
    )]
    Export,
}

impl DeclareVerb {
    /// Whether this builtin grants an attribute by virtue of its own name (`export` grants `-x`,
    /// `readonly` grants `-r`) rather than taking every attribute from an option, as `declare`
    /// and `local` do.
    ///
    /// Almost everything that separates `export` and `readonly` from `declare` follows from
    /// this, so this one predicate stands in for all of it:
    ///
    /// - the granted attribute is not an option, so `-p` alongside operands has nothing to
    ///   select and is a no-op, and `-f` applies the attribute instead of displaying;
    /// - `-a`/`-A` become modifiers of an assignment rather than the point of the command: they
    ///   apply only to an operand that assigns a value, may be combined, and `-a` wins;
    /// - a subscripted operand is an invalid identifier;
    /// - a missing function under `-f` is reported;
    /// - each assignment performed is echoed as an extra `set -x` line;
    /// - a readonly variable is reported the way a bare assignment reports it, without naming
    ///   the builtin.
    const fn implies_attribute(self) -> bool {
        matches!(self, Self::Readonly | Self::Export)
    }
}

#[derive(Clone, Copy)]
struct DeclarationScope {
    lookup: EnvironmentLookup,
    creation: EnvironmentScope,
}

/// A declaration whose expansion and structural interpretation are complete.
///
/// A shell applies what it can before it complains, so preparing an operand can succeed and
/// still have found something wrong: [`Self::stopped_by`] carries that error until the rest of
/// the operand has been applied, in the same two-phase shape as
/// [`ResolvedAssignment::stopped_by`], which is where most of them come from.
struct PreparedDeclaration {
    /// The variable being declared.
    name: String,
    /// The subscript the operand named, if any (present even when nothing is assigned, as in
    /// `declare arr[5]`), resolved to the element's final index or key.
    subscript: Option<String>,
    /// The subscript as the operand wrote it, for diagnostics. Differs from `subscript` only when
    /// resolution changed it (`a[i+1]` to `a[2]`).
    written_subscript: Option<String>,
    /// The value to assign, if any.
    initial_value: Option<ShellValueLiteral>,
    /// Whether the operand appended rather than replaced.
    append: bool,
    /// Whether the operand is unquoted compound syntax (`name=(...)`). A shell's refusal to
    /// assign such an operand is an assignment error rather than a builtin failure: nothing is
    /// granted and the command list is abandoned. (Only a refusal -- see
    /// [`brush_core::ErrorKind::is_assignment_failure`].)
    is_compound_syntax: bool,
    /// The array kind the target has before this declaration, if it exists and is an array.
    current_kind: Option<ArrayKind>,
    /// The array kind the declaration converts its target to, if any. See
    /// [`DeclareCommand::conversion_kind`].
    conversion: Option<ArrayKind>,
    /// An error that stopped this operand short while it was prepared -- a bad subscript on
    /// its name, or a bad key in its compound value -- raised once what was kept has been
    /// applied.
    stopped_by: Option<brush_core::Error>,
}

impl PreparedDeclaration {
    /// A declaration that names a variable without assigning to it. Other shapes are built from
    /// this one with struct update syntax.
    fn bare(name: &str, subscript: Option<&str>) -> Self {
        Self {
            name: name.to_owned(),
            subscript: subscript.map(str::to_owned),
            written_subscript: subscript.map(str::to_owned),
            initial_value: None,
            append: false,
            is_compound_syntax: false,
            current_kind: None,
            conversion: None,
            stopped_by: None,
        }
    }

    /// A declaration that binds its target as an array without assigning to it, which is how a
    /// shell binds a target whose subscript turned out to be bad.
    ///
    /// # Arguments
    ///
    /// * `name` - The variable being declared.
    /// * `current_kind` - The array kind the target already has, if it exists and is an array.
    fn bound_as_array(name: &str, current_kind: Option<ArrayKind>) -> Self {
        match current_kind {
            // A target that is already an array needs no binding, and must not get one:
            // appending to a declared-but-unset array would fill it out and wrongly leave it set.
            Some(_) => Self::bare(name, None),
            // Appending an empty list is how a shell binds a target as an array without giving
            // it a value: a new variable becomes a set, empty array, and a scalar is promoted to
            // element 0.
            None => Self {
                initial_value: Some(ShellValueLiteral::Array(variables::ArrayLiteral(vec![]))),
                append: true,
                ..Self::bare(name, None)
            },
        }
    }

    /// Returns the text `readonly` and `export` echo as an extra `set -x` trace line for this
    /// declaration: only a scalar assignment to a whole, validly named variable is echoed.
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
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        self.execute_as(DeclareVerb::Declare, &self.declarations, context)
            .await
    }
}

impl DeclareCommand {
    /// Executes this command as the given declaration builtin.
    ///
    /// # Arguments
    ///
    /// * `verb` - The builtin performing the declaration.
    /// * `declarations` - The operands to process. (Taken separately from `self` so a wrapper
    ///   builtin can lend its own without copying them.)
    /// * `context` - The execution context.
    pub(crate) async fn execute_as<SE: brush_core::ShellExtensions>(
        &self,
        verb: DeclareVerb,
        declarations: &[brush_core::CommandArg],
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        if !verb.implies_attribute()
            && self.make_indexed_array.to_bool() == Some(true)
            && self.make_associative_array.to_bool() == Some(true)
        {
            writeln!(
                context.stderr(),
                "{}: -a: invalid option",
                context.command_name
            )?;
            return Ok(ExecutionResult::new(2));
        }

        if matches!(verb, DeclareVerb::Local) && !context.shell.in_function() {
            writeln!(
                context.stderr(),
                "{}: can only be used in a function",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }

        let for_functions = self.function_names_only || self.function_names_or_defs_only;
        let mut result = ExecutionResult::success();
        if !declarations.is_empty() {
            // Operands are displayed, applied to functions, or applied to variables. `-p`
            // selects display and `-f`/`-F` select functions, which are displayed unless an
            // attribute is being applied to them.
            let display = self.print && !verb.implies_attribute();
            if display || for_functions {
                let applies_function_attributes = for_functions
                    && (verb.implies_attribute()
                        || self.make_traced.is_some()
                        || self.make_exported.is_some()
                        || self.make_readonly.is_some());

                for declaration in declarations {
                    // A function cannot be declared by assignment.
                    if for_functions && matches!(declaration, brush_core::CommandArg::Assignment(_))
                    {
                        writeln!(
                            context.stderr(),
                            "{}: cannot use `-f' to make functions",
                            context.command_name
                        )?;
                        result = ExecutionResult::general_error();
                        continue;
                    }

                    let succeeded = if applies_function_attributes {
                        self.apply_function_attributes(&mut context, declaration, verb)?
                    } else {
                        self.try_display_declaration(&context, declaration, verb)?
                    };
                    if !succeeded {
                        result = ExecutionResult::general_error();
                    }
                }
            } else {
                let scope = self.declaration_scope(&context, verb);

                // Operands are processed in order, each against the environment its
                // predecessors left behind. An assignment error propagates, so the interpreter
                // abandons the rest of the command list.
                for declaration in declarations {
                    let prepared = self
                        .prepare_declaration(&mut context, declaration, verb, scope)
                        .await?;

                    // `export` and `readonly` echo each assignment they perform as a trace
                    // line of its own, on top of the one the interpreter already wrote for the
                    // command.
                    if verb.implies_attribute()
                        && let Some(line) = prepared.render_traced_assignment()
                    {
                        context.trace_extra_line(line).await;
                    }

                    if !self.apply_declaration(&mut context, prepared, verb, scope)? {
                        result = ExecutionResult::general_error();
                    }
                }
            }
        } else {
            if !for_functions {
                self.display_matching_env_declarations(&context, verb)?;
            }

            // Functions are listed under -f/-F, and otherwise only when nothing selected
            // variables specifically: `-p`, an attribute option, or a verb that implies one.
            if !matches!(verb, DeclareVerb::Local)
                && (for_functions
                    || (!self.print
                        && !verb.implies_attribute()
                        && self.attribute_selectors().is_empty()))
            {
                self.display_matching_functions(&context, verb)?;
            }
        }

        Ok(result)
    }

    /// Resolves the lookup and creation scopes for this invocation.
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

    fn apply_function_attributes(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
        verb: DeclareVerb,
    ) -> Result<bool, brush_core::Error> {
        let func = match declaration {
            brush_core::CommandArg::String(name) => context.shell.func_mut(name),
            brush_core::CommandArg::Assignment(_) => None,
        };

        let Some(func) = func else {
            if verb.implies_attribute() {
                writeln!(
                    context.stderr(),
                    "{}: {declaration}: not a function",
                    context.command_name
                )?;
            }
            return Ok(false);
        };

        if self.make_readonly.to_bool() == Some(false) && func.is_readonly() {
            writeln!(
                context.stderr(),
                "{}: {declaration}: readonly function",
                context.command_name
            )?;
            return Ok(false);
        }

        match self.make_exported.to_bool() {
            Some(true) => func.export(),
            Some(false) => func.unexport(),
            None => (),
        }
        match self.make_traced.to_bool() {
            Some(true) => func.enable_trace(),
            Some(false) => func.disable_trace(),
            None => (),
        }
        if matches!(verb, DeclareVerb::Readonly) || self.make_readonly.to_bool() == Some(true) {
            func.set_readonly();
        }

        Ok(true)
    }

    /// Applies one prepared declaration to the variable environment. Returns `true` on success,
    /// or `false` for a failure that affects the exit status without stopping the remaining
    /// operands.
    fn apply_declaration(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: PreparedDeclaration,
        verb: DeclareVerb,
        scope: DeclarationScope,
    ) -> Result<bool, brush_core::Error> {
        // `+a`/`+A` cannot remove an array attribute, even from a declared-but-unset array.
        let dropping = match declaration.current_kind {
            Some(ArrayKind::Indexed) => self.make_indexed_array.to_bool() == Some(false),
            Some(ArrayKind::Associative) => self.make_associative_array.to_bool() == Some(false),
            None => false,
        };
        if dropping {
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

        if !env::valid_variable_name(declaration.name.as_str()) {
            writeln!(
                context.stderr(),
                "{}: `{}': not a valid identifier",
                context.command_name,
                declaration.name,
            )?;
            return Ok(false);
        }

        if verb.implies_attribute()
            && let Some(subscript) = &declaration.subscript
        {
            writeln!(
                context.stderr(),
                "{}: `{}[{subscript}]': not a valid identifier",
                context.command_name,
                declaration.name,
            )?;
            return Ok(false);
        }

        // A failure is reported against the variable as written. An assignment error
        // propagates; any other failure fails just this operand.
        let name = declaration.name.clone();
        let subscript = declaration.written_subscript.clone();
        match self.update_declared_variable(context, declaration, verb, scope) {
            Ok(()) => Ok(true),
            Err(err) => {
                let err = err.for_variable(&name, subscript.as_deref());
                if err.is_assignment_error() {
                    Err(err)
                } else {
                    self.report_recoverable_error(context, err, verb)
                }
            }
        }
    }

    /// Reports a recoverable per-operand failure to stderr and returns `Ok(false)`; any other
    /// error propagates.
    fn report_recoverable_error(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        err: brush_core::Error,
        verb: DeclareVerb,
    ) -> Result<bool, brush_core::Error> {
        match err.kind() {
            // Plain `export` and `readonly` report a readonly variable the way a bare assignment
            // does, without naming the builtin; a bad subscript is always reported bare.
            ErrorKind::ReadonlyVariable
                if verb.implies_attribute() && self.requested_array_kind().is_none() =>
            {
                writeln!(context.stderr(), "{err}")?;
            }
            ErrorKind::BadArraySubscript(_) => writeln!(context.stderr(), "{err}")?,
            ErrorKind::ReadonlyVariable
            | ErrorKind::ConvertingIndexedArrayToAssociativeArray
            | ErrorKind::ConvertingAssociativeArrayToIndexedArray => {
                writeln!(context.stderr(), "{}: {err}", context.command_name)?;
            }
            _ => return Err(err),
        }

        Ok(false)
    }

    /// Applies one prepared declaration to the environment, updating the variable it names or
    /// creating it.
    fn update_declared_variable(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: PreparedDeclaration,
        verb: DeclareVerb,
        scope: DeclarationScope,
    ) -> Result<(), brush_core::Error> {
        // Read before the env borrow below is taken out.
        let auto_export = context.shell.options().export_variables_on_modification;

        // `local -I` / `declare -I` (bash 5.0+) starts the new local from a copy of the nearest
        // same-name variable, wherever it lives. The copy is bound first and then updated like
        // any existing variable, so a kind conflict still leaves the local in place. A readonly
        // inheritee refuses the declaration outright.
        let inheritee = self.inheritee(context.shell.env(), &declaration.name, scope);
        let inherited = if let Some(inheritee) = inheritee {
            if inheritee.is_readonly() {
                return Err(ErrorKind::ReadonlyVariable.into());
            }

            let mut var = inheritee.clone();
            // Inherit what a dynamic value (DIRSTACK and friends) resolves to, and inherit a
            // nameref's target string as an ordinary scalar.
            var.resolve_dynamic(context.shell);
            var.unset_treat_as_nameref();

            context
                .shell
                .env_mut()
                .add(declaration.name.clone(), var, scope.creation)?;
            true
        } else {
            false
        };

        if let Some(var) = context
            .shell
            .env_mut()
            .get_mut_using_policy(declaration.name.as_str(), scope.lookup)
        {
            // A shell discards a function-local scalar's value on conversion; only a global
            // keeps it as element 0.
            let policy = if matches!(scope.creation, EnvironmentScope::Local) {
                ScalarConversionPolicy::Discard
            } else {
                ScalarConversionPolicy::PromoteToElementZero
            };
            let conversion = declaration.conversion.map(|kind| (kind, policy));

            // `set -a` exports a new variable outright, but an existing one only when the
            // declaration assigns to it.
            let export = auto_export && (inherited || declaration.initial_value.is_some());
            self.update_variable(var, declaration, verb, conversion, export)?;
        } else {
            // `export -n` removes an attribute; unless it also assigns, it has nothing to create.
            if matches!(verb, DeclareVerb::Export)
                && self.make_exported.to_bool() == Some(false)
                && declaration.initial_value.is_none()
            {
                return Ok(());
            }

            // A local may not shadow a readonly global (a readonly local in an enclosing
            // function's scope is fine).
            if matches!(scope.creation, EnvironmentScope::Local)
                && context
                    .shell
                    .env()
                    .get_using_policy(&declaration.name, EnvironmentLookup::OnlyInGlobal)
                    .is_some_and(ShellVariable::is_readonly)
            {
                return Err(ErrorKind::ReadonlyVariable.into());
            }

            let mut var = ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped));
            // The value is unset, so the scalar policy is moot.
            let conversion = declaration
                .conversion
                .map(|kind| (kind, ScalarConversionPolicy::Discard));

            // The variable is bound even when assigning to it fails, so a failed element
            // assignment still leaves it declared with its kind and no value.
            let name = declaration.name.clone();
            let updated =
                self.update_variable(&mut var, declaration, verb, conversion, auto_export);
            context.shell.env_mut().add(name, var, scope.creation)?;
            updated?;
        }

        Ok(())
    }

    /// The shared tail of a declaration: converts the variable's kind, assigns any value, and
    /// applies attributes, in the order a shell does.
    ///
    /// The value update runs first (kind conversion, the readonly check, the attributes that
    /// shape how a value is stored, then the value). Two separate things can then have gone
    /// wrong, and they grant different amounts:
    ///
    /// | The update | ...and what was recorded in `prepare` | Verb's own attribute | Option attributes, `set -a` |
    /// |---|---|---|---|
    /// | succeeded | nothing | granted | granted |
    /// | succeeded | a bad subscript on the name | granted | granted |
    /// | failed | (either way) | granted | not granted |
    ///
    /// So `declare -rx 'b[*]=1'` still leaves `b` an empty readonly, exported array, while a
    /// refused conversion grants only `export`'s `-x` or `readonly`'s `-r`.
    ///
    /// An unquoted compound operand is the exception to the whole table: any failed outcome is
    /// an assignment error, none of these attributes is granted, and the error propagates. The
    /// attributes that shape how a value is stored are applied earlier and survive regardless --
    /// see [`Self::apply_pre_assignment_attributes`].
    ///
    /// `set -a` never exports an array, outranks an explicit `+x`, but yields to `export -n`.
    ///
    /// # Arguments
    ///
    /// * `var` - The variable to update.
    /// * `declaration` - The declaration to apply.
    /// * `verb` - The builtin performing the declaration.
    /// * `conversion` - The array kind to convert the variable to first, if any, with the policy
    ///   for a set scalar value.
    /// * `auto_export` - Whether `set -a` applies to this update.
    fn update_variable(
        &self,
        var: &mut ShellVariable,
        mut declaration: PreparedDeclaration,
        verb: DeclareVerb,
        conversion: Option<(ArrayKind, ScalarConversionPolicy)>,
        auto_export: bool,
    ) -> Result<(), brush_core::Error> {
        let is_compound_syntax = declaration.is_compound_syntax;
        let stopped_by = declaration.stopped_by.take();
        let updated = self.update_value(var, declaration, conversion);
        let value_assigned = updated.is_ok();
        let outcome = updated.and(stopped_by.map_or(Ok(()), Err));
        // Only a shell's own refusal to assign becomes an assignment error; brush failing to
        // carry the assignment out (an unimplemented case, say) fails the operand like any
        // other error rather than abandoning the caller's command list.
        if let Err(err) = &outcome
            && is_compound_syntax
            && err.kind().is_assignment_failure()
        {
            return outcome.map_err(brush_core::Error::into_assignment_error);
        }

        let implied_export = matches!(verb, DeclareVerb::Export);
        match verb {
            DeclareVerb::Export => self.apply_export_flag(var),
            DeclareVerb::Readonly => {
                var.set_readonly();
            }
            DeclareVerb::Declare | DeclareVerb::Local => (),
        }

        if value_assigned {
            if !implied_export {
                self.apply_export_flag(var);
            }
            self.apply_trace_flag(var);
            self.apply_readonly_flag(var)?;

            let auto_export =
                auto_export && !(implied_export && self.make_exported.to_bool() == Some(false));
            if auto_export && !var.value().is_array() {
                var.export();
            }
        }

        outcome
    }

    /// Performs the value half of a declaration: the kind conversion, the option attributes that
    /// shape how a value is assigned, and the assignment itself. Everything that can refuse the
    /// update is checked before the variable is touched, so a refused operand leaves no
    /// attribute behind.
    fn update_value(
        &self,
        var: &mut ShellVariable,
        declaration: PreparedDeclaration,
        conversion: Option<(ArrayKind, ScalarConversionPolicy)>,
    ) -> Result<(), brush_core::Error> {
        if let Some((kind, policy)) = conversion {
            var.convert_to_array_kind(kind, policy)?;
        }

        if var.is_readonly()
            && (declaration.initial_value.is_some() || self.requests_value_transform())
        {
            return Err(ErrorKind::ReadonlyVariable.into());
        }

        self.apply_pre_assignment_attributes(var);

        if let Some(initial_value) = declaration.initial_value {
            var.assign_at(declaration.subscript, initial_value, declaration.append)?;
        }

        Ok(())
    }

    /// Returns the array kind a declaration converts its target to, if any. An explicit
    /// `-a`/`-A` always converts (for `export`/`readonly`, only alongside a value). Otherwise an
    /// operand whose shape implies an array -- a subscripted name or a compound value -- makes
    /// an indexed array of a target that is not one already.
    ///
    /// # Arguments
    ///
    /// * `assigns_value` - Whether the operand assigns a value.
    /// * `implies_array` - Whether the operand's shape implies an array target.
    /// * `verb` - The builtin performing the declaration.
    /// * `current` - The array kind the target currently has, if it exists and is an array.
    fn conversion_kind(
        &self,
        assigns_value: bool,
        implies_array: bool,
        verb: DeclareVerb,
        current: Option<ArrayKind>,
    ) -> Option<ArrayKind> {
        if verb.implies_attribute() && !assigns_value {
            return None;
        }

        self.requested_array_kind()
            .or_else(|| (current.is_none() && implies_array).then_some(ArrayKind::Indexed))
    }

    /// Returns the variable a `-I` declaration would start the new local from: the nearest
    /// same-name variable, wherever it lives. `None` when this invocation did not ask to
    /// inherit, is not creating a local, or no such variable exists.
    fn inheritee<'a>(
        &self,
        env: &'a env::ShellEnvironment,
        name: &str,
        scope: DeclarationScope,
    ) -> Option<&'a ShellVariable> {
        if self.locals_inherit_from_prev_scope.is_some()
            && matches!(scope.creation, EnvironmentScope::Local)
        {
            env.get_using_policy(name, EnvironmentLookup::Anywhere)
        } else {
            None
        }
    }

    /// Returns the current array kind of the variable this declaration will update: the one a
    /// `-I` declaration inherits, or else one already in the declaration's own scope. `None`
    /// when no such variable exists or it is not an array. A dynamic value answers with the
    /// kind of what it resolves to.
    fn current_target_kind(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        name: &str,
        scope: DeclarationScope,
    ) -> Option<ArrayKind> {
        let env = context.shell.env();
        self.inheritee(env, name, scope)
            .or_else(|| env.get_using_policy(name, scope.lookup))
            .and_then(|var| match var.value() {
                ShellValue::Dynamic { .. } => var.resolve_value(context.shell).array_kind(),
                value => value.array_kind(),
            })
    }

    /// Prepares one operand for application: interprets it, decides the array conversion it
    /// calls for, and resolves its subscripts against the resulting kind.
    async fn prepare_declaration(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        declaration: &brush_core::CommandArg,
        verb: DeclareVerb,
        scope: DeclarationScope,
    ) -> Result<PreparedDeclaration, brush_core::Error> {
        // Assignment syntax wins over a bare name: it is the only interpretation under which
        // the text after `=` is a value, and checking it first keeps a value that merely ends
        // in `]` (`x=[a]`) from being mistaken for a subscripted name. Quoting or an expansion
        // may have hidden it from the parser; the operand as a whole was already expanded, so
        // such a value is taken verbatim.
        let assignment = match declaration {
            brush_core::CommandArg::Assignment(assignment) => Cow::Borrowed(assignment),
            brush_core::CommandArg::String(operand) => {
                match brush_parser::word::parse_scalar_assignment(
                    operand,
                    &context.shell.parser_options(),
                ) {
                    Ok(assignment) => Cow::Owned(assignment),
                    Err(_) => return Ok(self.prepare_bare_operand(context, operand, verb, scope)),
                }
            }
        };

        // Only a parser-recognized compound operand is compound syntax; a quoted one
        // (`'a=(1 2)'`) is a scalar until reinterpreted below.
        let is_compound_syntax = matches!(declaration, brush_core::CommandArg::Assignment(_))
            && matches!(assignment.value, ast::AssignmentValue::Array(_));

        // A rejected subscripted operand (see apply_declaration) is reported as written: its
        // subscript is never evaluated and its value never assigned.
        if verb.implies_attribute()
            && let ast::AssignmentName::ArrayElementName(name, subscript) = &assignment.name
        {
            return Ok(PreparedDeclaration {
                is_compound_syntax,
                ..PreparedDeclaration::bare(name, Some(subscript))
            });
        }

        // Subscripts resolve against the kind the target will have once this declaration has
        // applied: the kind it converts to, else the kind it already has, else indexed.
        let name = assignment.name.base_name();
        let current_kind = self.current_target_kind(context, name, scope);
        let implies_array = matches!(assignment.name, ast::AssignmentName::ArrayElementName(..))
            || matches!(assignment.value, ast::AssignmentValue::Array(_));
        let conversion = self.conversion_kind(true, implies_array, verb, current_kind);
        let target = conversion.or(current_kind).unwrap_or(ArrayKind::Indexed);

        let name = name.to_owned();
        let written_subscript = match &assignment.name {
            ast::AssignmentName::VariableName(_) => None,
            ast::AssignmentName::ArrayElementName(_, index) => Some(index.clone()),
        };
        let resolved = context
            .shell
            .resolve_assignment_subscripts(&context.params, assignment.into_owned(), target)
            .await;
        let ResolvedAssignment {
            assignment,
            stopped_by,
        } = match resolved {
            Ok(resolved) => resolved,
            // A bad subscript on the name: a shell still binds the target as an array (leaving
            // one that is already an array exactly as it was) and then fails the operand.
            Err(err) if matches!(err.kind(), ErrorKind::BadArraySubscript(_)) => {
                return Ok(PreparedDeclaration {
                    is_compound_syntax,
                    current_kind,
                    conversion,
                    stopped_by: Some(err),
                    ..PreparedDeclaration::bound_as_array(&name, current_kind)
                });
            }
            Err(err) => return Err(err),
        };

        // A value that only now looks like compound syntax is reinterpreted. A bad key in it is
        // only a warning, as in a shell: the value stops short, but the operand succeeds.
        let assignment = match self
            .reinterpret_as_compound(context, &assignment, current_kind.is_some(), target)
            .await?
        {
            Some(reinterpreted) => {
                if let Some(err) = reinterpreted.stopped_by {
                    writeln!(context.stderr(), "{err}")?;
                }
                reinterpreted.assignment
            }
            None => assignment,
        };

        let (name, subscript) = match assignment.name {
            ast::AssignmentName::VariableName(name) => (name, None),
            ast::AssignmentName::ArrayElementName(name, index) => (name, Some(index)),
        };
        Ok(PreparedDeclaration {
            name,
            subscript,
            written_subscript,
            initial_value: Some(assignment.value.into()),
            append: assignment.append,
            is_compound_syntax,
            current_kind,
            conversion,
            stopped_by,
        })
    }

    /// Prepares an operand holding no assignment syntax. `declare array[index]` names an array
    /// without assigning to it; the subscript only marks the operand as an array declaration
    /// and is never evaluated. An empty subscript, or one followed by more text (`a[1][2]`), is
    /// left in the name so that it fails as an invalid identifier.
    fn prepare_bare_operand(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        operand: &str,
        verb: DeclareVerb,
        scope: DeclarationScope,
    ) -> PreparedDeclaration {
        let (name, subscript) = match operand.strip_suffix(']').and_then(|s| s.split_once('[')) {
            Some((name, subscript)) if !subscript.is_empty() && !subscript.contains(']') => {
                (name, Some(subscript))
            }
            _ => (operand, None),
        };

        let current_kind = self.current_target_kind(context, name, scope);
        PreparedDeclaration {
            current_kind,
            conversion: self.conversion_kind(false, subscript.is_some(), verb, current_kind),
            ..PreparedDeclaration::bare(name, subscript)
        }
    }

    /// Returns the array kind this invocation explicitly requested with `-a` or `-A`, if any.
    /// When both are given (which only `export` and `readonly` allow), `-a` wins.
    fn requested_array_kind(&self) -> Option<ArrayKind> {
        if self.make_indexed_array.to_bool() == Some(true) {
            Some(ArrayKind::Indexed)
        } else if self.make_associative_array.to_bool() == Some(true) {
            Some(ArrayKind::Associative)
        } else {
            None
        }
    }

    /// Reinterprets an expanded assignment's scalar value as a compound array value when the
    /// requested attributes or the target's existing type call for it. Returns `None` if the
    /// value should stay scalar.
    async fn reinterpret_as_compound(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        assignment: &ast::Assignment,
        target_is_array: bool,
        target: ArrayKind,
    ) -> Result<Option<ResolvedAssignment>, brush_core::Error> {
        // Without an array attribute or an already-array target, the text stays scalar. A
        // subscripted operand is reinterpreted only under an explicit attribute: an existing
        // array's element takes the text literally.
        let subscripted = matches!(assignment.name, ast::AssignmentName::ArrayElementName(..));
        if self.requested_array_kind().is_none() && (subscripted || !target_is_array) {
            return Ok(None);
        }

        // Parser-recognized compound assignments already had their elements expanded.
        let ast::AssignmentValue::Scalar(value) = &assignment.value else {
            return Ok(None);
        };

        let Some(elements) = brush_parser::word::parse_compound_assignment_value(
            value.value.as_str(),
            &context.shell.parser_options(),
        ) else {
            return Ok(None);
        };

        // The compound syntax hid the elements from the operand's expansion, so they are
        // expanded now, exactly once. A compound value cannot target a single element, so the
        // subscript is dropped and the whole array assigned.
        let compound = ast::Assignment {
            name: ast::AssignmentName::VariableName(assignment.name.base_name().to_owned()),
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

    /// Returns whether an option asks for an attribute that can transform the variable's value:
    /// the integer and case transforms (added or removed), or becoming a nameref. A readonly
    /// variable refuses these; `+n` and the pure flags (`-x`/`-t`) stay permitted.
    fn requests_value_transform(&self) -> bool {
        self.make_integer.is_some()
            || self.capitalize_value_on_assignment.is_some()
            || self.lowercase_value_on_assignment.is_some()
            || self.uppercase_value_on_assignment.is_some()
            || self.make_nameref.to_bool() == Some(true)
    }

    /// Applies the option attributes that have to be on the variable before a value is: the ones
    /// that shape how the value is stored (`-i`, `-c`/`-l`/`-u`, `-n`). A shell keeps these even
    /// when the assignment then fails, which is what separates them from the flags applied after.
    const fn apply_pre_assignment_attributes(&self, var: &mut ShellVariable) {
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
    }

    /// Applies the `-x`/`+x` flag, if given.
    const fn apply_export_flag(&self, var: &mut ShellVariable) {
        match self.make_exported.to_bool() {
            Some(true) => {
                var.export();
            }
            Some(false) => {
                var.unexport();
            }
            None => (),
        }
    }

    /// Applies the `-t`/`+t` flag, if given. It shapes nothing about the value, so like `-x` and
    /// `-r` it is granted only once the assignment has gone through.
    const fn apply_trace_flag(&self, var: &mut ShellVariable) {
        match self.make_traced.to_bool() {
            Some(true) => {
                var.enable_trace();
            }
            Some(false) => {
                var.disable_trace();
            }
            None => (),
        }
    }

    /// Applies the `-r`/`+r` flag, if given. Errors if readonly status cannot be removed.
    fn apply_readonly_flag(&self, var: &mut ShellVariable) -> Result<(), brush_core::Error> {
        match self.make_readonly.to_bool() {
            Some(true) => {
                var.set_readonly();
            }
            Some(false) => {
                var.unset_readonly()?;
            }
            None => (),
        }

        Ok(())
    }
}
