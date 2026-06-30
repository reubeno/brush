//! Mutation methods on [`ShellEnvironment`]: `unset`, `set_var`, the
//! [`VarWrite`] builder (via [`ShellEnvironment::write`]), and `add`. The lookup
//! and resolution APIs live in `mod.rs` and `lookup.rs`; this file is the
//! write-side counterpart.

use super::names::{NameRefStrategy, ResolvedScope};
use super::{
    EnvironmentLookup, EnvironmentScope, NameRef, ResolvedName, ShellEnvironment, ShellVariableMap,
};
use crate::error;
use crate::variables::{self, ShellValue, ShellValueUnsetType, ShellVariable};

/// A validated write target derived from a [`NameRef`]: the base variable name,
/// an optional array subscript, and the scope the name resolves against. This is
/// the single shape every mutation path resolves to before touching storage.
struct WriteTarget {
    base: String,
    subscript: Option<String>,
    scope: ResolvedScope,
}

/// Post-assignment hook stored by [`VarWrite::updating`].
type VarUpdater<'a> = Box<dyn Fn(&mut ShellVariable) -> Result<(), error::Error> + 'a>;

/// Fluent builder for a variable write, created by [`ShellEnvironment::write`] —
/// the mutating counterpart to the [`lookup`](ShellEnvironment::lookup) read
/// builders.
///
/// Collects optional modifiers — an array element [`at_index`](Self::at_index),
/// a post-assignment [`updating`](Self::updating) hook, a lookup
/// [`in_scope`](Self::in_scope) restriction, and the
/// [`creating_in`](Self::creating_in) scope for a freshly-created variable —
/// then performs the write on [`set`](Self::set). Nameref resolution follows the
/// [`NameRef`] the builder was created from.
#[must_use = "a VarWrite performs no write until `.set(value)` is called"]
pub struct VarWrite<'a> {
    env: &'a mut ShellEnvironment,
    name: NameRef,
    index: Option<String>,
    updater: Option<VarUpdater<'a>>,
    lookup_policy: EnvironmentLookup,
    scope_if_creating: EnvironmentScope,
}

impl ShellEnvironment {
    /// Resolves a [`NameRef`] to a validated [`WriteTarget`], the shared first
    /// step of every mutation. Walks the nameref chain only when `name` is a
    /// live nameref (fast path otherwise), parses any subscript, and rejects a
    /// base that isn't a legal identifier — so `read 'foo['` / `printf -v 1bad`
    /// fail here rather than corrupting the map.
    fn resolve_write_target(&self, name: NameRef) -> Result<WriteTarget, error::Error> {
        let (base, subscript, scope) = match name.0 {
            NameRefStrategy::Resolve(s) => {
                // Fast path: only walk the chain when `s` is a live nameref. The
                // common case (a plain counter, `read`/`getopts`/`mapfile`
                // target) skips the walk and the resolve allocation, but still
                // parses a possible subscript (e.g. `read 'arr[0]'`).
                if self
                    .get_by_exact_name(&s)
                    .is_some_and(|(_, v)| v.is_treated_as_nameref())
                {
                    let resolved = self.resolve_nameref(&s)?;
                    (resolved.name, resolved.subscript, resolved.scope)
                } else {
                    let resolved = ResolvedName::parse(s);
                    (resolved.name, resolved.subscript, ResolvedScope::Default)
                }
            }
            NameRefStrategy::PreResolved(r) => (r.name, r.subscript, r.scope),
            NameRefStrategy::Bypass(s) => (s, None, ResolvedScope::Default),
        };

        if !super::names::valid_variable_name(&base) {
            return Err(error::ErrorKind::InvalidVariableName(base).into());
        }

        Ok(WriteTarget {
            base,
            subscript,
            scope,
        })
    }
    /// Unsets a variable from the environment.
    ///
    /// Resolution strategy depends on the [`NameRef`] variant:
    /// - `Resolve` (default for `&str`) — follows nameref chains. On circular
    ///   namerefs, falls back to unsetting the variable itself.
    /// - `PreResolved` — unsets by the resolved base name; if a subscript
    ///   is present, only the array element is removed.
    /// - `Bypass` — unsets the variable itself, bypassing namerefs.
    ///
    /// Returns the removed [`ShellVariable`] when a whole variable is unset, or `None`
    /// if the variable was not found or only an array element was removed.
    pub fn unset(
        &mut self,
        name: impl Into<NameRef>,
    ) -> Result<Option<ShellVariable>, error::Error> {
        let strategy = name.into().0;
        // A nameref fault (cycle / max-depth) means bash unsets the head variable
        // itself, so a Resolve that faults falls back to the raw name. Bypass and
        // PreResolved never fault, so resolution is infallible for them.
        let target = match strategy {
            NameRefStrategy::Resolve(s) => match self.resolve_write_target(NameRef::resolve(&s)) {
                Ok(t) => t,
                Err(_) => return self.unset_raw(&s),
            },
            other => self.resolve_write_target(NameRef(other))?,
        };
        if let Some(idx) = target.subscript {
            if let Some((_, var)) = self.get_mut_by_exact_name(&target.base) {
                var.unset_index(&idx)?;
            }
            return Ok(None);
        }
        self.unset_raw(&target.base)
    }

    /// Internal: unset by raw string name, no nameref resolution.
    fn unset_raw(&mut self, name: &str) -> Result<Option<ShellVariable>, error::Error> {
        let mut local_count = 0;
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            let unset_result = try_unset_in_map(map, name)?;

            if unset_result.is_some() {
                // If we end up finding a local in the top-most local frame, then we replace
                // it with a placeholder.
                if matches!(scope_type, EnvironmentScope::Local) && local_count == 1 {
                    map.set(
                        name,
                        ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped)),
                    );
                } else if self.entry_count > 0 {
                    // Entry count should never be 0 here, but we're being defensive.
                    self.entry_count -= 1;
                }

                return Ok(unset_result);
            }
        }

        Ok(None)
    }

    /// Update a variable in the environment, or add it if it doesn't already exist.
    ///
    /// Resolution strategy depends on the [`NameRef`] variant:
    /// - `Resolve` (default for `&str`) — follows nameref chains before writing.
    /// - `PreResolved` — uses the resolved base name; if a subscript is present,
    ///   redirects scalar values to array-element assignment.
    /// - `Bypass` — writes to the variable itself, bypassing namerefs.
    ///
    /// Internal lowering target for [`set_var`](Self::set_var) and the
    /// [`write`](Self::write) builder.
    fn update_or_add(
        &mut self,
        name: impl Into<NameRef>,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let target = self.resolve_write_target(name.into())?;

        // A self-referential nameref (`local -n x=x`) resolves to the global
        // `x`: `ResolvedScope::Global` forces global lookup and creation,
        // overriding the caller's policy/scope. (Single source of truth.)
        let lookup_policy = target.scope.lookup_policy_or(lookup_policy);
        let scope_if_creating = target.scope.creation_scope_or(scope_if_creating);

        // A subscripted target writes the element for a scalar; compound (array)
        // assignment through it is rejected by bash as "not a valid identifier".
        if let Some(idx) = target.subscript {
            return match value {
                variables::ShellValueLiteral::Scalar(scalar) => self
                    .update_or_add_array_element_impl(
                        target.base,
                        idx,
                        scalar,
                        updater,
                        lookup_policy,
                        scope_if_creating,
                    ),
                variables::ShellValueLiteral::Array(_) => {
                    Err(error::ErrorKind::SubscriptedNameRefTarget {
                        name: target.base,
                        subscript: idx,
                    }
                    .into())
                }
            };
        }

        self.update_or_add_impl(
            target.base,
            value,
            updater,
            lookup_policy,
            scope_if_creating,
        )
    }

    /// Convenience for the common assignment: no post-update attribute tweak,
    /// look anywhere, create in the global scope. Accepts anything that names a
    /// variable — a `&str`/`String` (resolves namerefs), a [`ResolvedName`], or
    /// a [`NameRef`] (e.g. [`NameRef::bypass`]) — and any value that converts
    /// into a [`ShellValueLiteral`](variables::ShellValueLiteral), so
    /// `set_var(name, "value")` needs no ceremony. For element writes or
    /// non-default scope/attribute handling, use [`write`](Self::write).
    ///
    /// [`ResolvedName`]: super::ResolvedName
    pub fn set_var(
        &mut self,
        name: impl Into<NameRef>,
        value: impl Into<variables::ShellValueLiteral>,
    ) -> Result<(), error::Error> {
        self.update_or_add(
            name,
            value.into(),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )
    }

    /// Begins a fluent variable write — the mutating counterpart to
    /// [`lookup`](Self::lookup). Defaults to a whole-variable assignment that
    /// looks anywhere and creates in the global scope; chain
    /// [`at_index`](VarWrite::at_index), [`updating`](VarWrite::updating),
    /// [`in_scope`](VarWrite::in_scope), or [`creating_in`](VarWrite::creating_in)
    /// to refine it, then finish with [`set`](VarWrite::set).
    ///
    /// `name` resolves namerefs by default; pass a [`NameRef`] for another
    /// strategy (e.g. [`NameRef::bypass`]).
    pub fn write(&mut self, name: impl Into<NameRef>) -> VarWrite<'_> {
        VarWrite {
            env: self,
            name: name.into(),
            index: None,
            updater: None,
            lookup_policy: EnvironmentLookup::Anywhere,
            scope_if_creating: EnvironmentScope::Global,
        }
    }

    fn update_or_add_impl(
        &mut self,
        name: String,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let auto_export = self.export_variables_on_modification;
        if let Some((_, var)) = self.get_mut_by_exact_name_using_policy(&name, lookup_policy) {
            if var.is_treated_as_nameref()
                && let variables::ShellValueLiteral::Scalar(target) = &value
                && !super::names::valid_nameref_target_name(target)
            {
                return Err(error::ErrorKind::InvalidVariableName(target.clone()).into());
            }
            var.assign(value, false)?;
            if auto_export {
                var.export();
            }
            updater(var)
        } else {
            let mut var = ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped));
            var.assign(value, false)?;
            if auto_export {
                var.export();
            }
            updater(&mut var)?;

            self.add(name, var, scope_if_creating)
        }
    }

    /// Update an array element in the environment, or add it if it doesn't already exist.
    ///
    /// Resolution strategy depends on the [`NameRef`] variant (same as
    /// `update_or_add`). The explicit `index` parameter always takes precedence
    /// over any subscript embedded in a nameref target. Internal lowering target
    /// for the [`write`](Self::write) builder's element path.
    fn update_or_add_array_element(
        &mut self,
        name: impl Into<NameRef>,
        index: String,
        value: String,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let target = self.resolve_write_target(name.into())?;

        // A target that already carries a subscript plus an explicit index would
        // be `arr[N][index]` — bash rejects it as not a valid identifier.
        if let Some(subscript) = target.subscript {
            return Err(error::ErrorKind::SubscriptedNameRefTarget {
                name: target.base,
                subscript,
            }
            .into());
        }

        // Self-referential nameref → global scope (see `update_or_add`).
        let lookup_policy = target.scope.lookup_policy_or(lookup_policy);
        let scope_if_creating = target.scope.creation_scope_or(scope_if_creating);

        self.update_or_add_array_element_impl(
            target.base,
            index,
            value,
            updater,
            lookup_policy,
            scope_if_creating,
        )
    }

    /// Internal: update an array element where the name has already been resolved.
    fn update_or_add_array_element_impl(
        &mut self,
        name: String,
        index: String,
        value: String,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        if let Some((_, var)) = self.get_mut_by_exact_name_using_policy(&name, lookup_policy) {
            var.assign_at_index(index, value, false)?;
            updater(var)
        } else {
            let mut var = ShellVariable::new(ShellValue::Unset(ShellValueUnsetType::Untyped));
            var.assign(
                variables::ShellValueLiteral::Array(variables::ArrayLiteral(vec![(
                    Some(index),
                    value,
                )])),
                false,
            )?;
            updater(&mut var)?;

            self.add(name, var, scope_if_creating)
        }
    }

    /// Adds a variable to the environment in the specified scope.
    pub fn add<N: Into<String>>(
        &mut self,
        name: N,
        mut var: ShellVariable,
        target_scope: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let name = name.into();
        if !super::names::valid_variable_name(&name) {
            return Err(error::ErrorKind::InvalidVariableName(name).into());
        }

        if self.export_variables_on_modification {
            var.export();
        }

        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if *scope_type == target_scope {
                let prev_var = map.set(name, var);
                if prev_var.is_none() {
                    self.entry_count += 1;
                }

                return Ok(());
            }
        }

        Err(error::ErrorKind::MissingScopeForNewVariable.into())
    }

    /// Sets a global variable in the environment.
    pub fn set_global<N: Into<String>>(
        &mut self,
        name: N,
        var: ShellVariable,
    ) -> Result<(), error::Error> {
        self.add(name, var, EnvironmentScope::Global)
    }
}

impl<'a> VarWrite<'a> {
    /// Targets the array element at `index` instead of the whole variable. The
    /// explicit index takes precedence over any subscript embedded in a nameref
    /// target.
    pub fn at_index(mut self, index: impl Into<String>) -> Self {
        self.index = Some(index.into());
        self
    }

    /// Runs `f` on the variable immediately after assignment — for attribute
    /// tweaks such as marking it exported or hidden from enumeration.
    pub fn updating(
        mut self,
        f: impl Fn(&mut ShellVariable) -> Result<(), error::Error> + 'a,
    ) -> Self {
        self.updater = Some(Box::new(f));
        self
    }

    /// Restricts which scope an *existing* variable is looked up in. Defaults to
    /// [`EnvironmentLookup::Anywhere`].
    pub const fn in_scope(mut self, policy: EnvironmentLookup) -> Self {
        self.lookup_policy = policy;
        self
    }

    /// Sets the scope a *newly created* variable lands in. Defaults to
    /// [`EnvironmentScope::Global`].
    pub const fn creating_in(mut self, scope: EnvironmentScope) -> Self {
        self.scope_if_creating = scope;
        self
    }

    /// Performs the write, assigning `value` and creating the variable if it
    /// doesn't yet exist. Assigning an array literal to an
    /// [`at_index`](Self::at_index) element target is rejected (bash forbids
    /// assigning a list to an array member).
    pub fn set(self, value: impl Into<variables::ShellValueLiteral>) -> Result<(), error::Error> {
        let Self {
            env,
            name,
            index,
            updater,
            lookup_policy,
            scope_if_creating,
        } = self;
        let updater = updater.unwrap_or_else(|| Box::new(|_| Ok(())));
        let value = value.into();
        match index {
            Some(index) => match value {
                variables::ShellValueLiteral::Scalar(scalar) => env.update_or_add_array_element(
                    name,
                    index,
                    scalar,
                    updater,
                    lookup_policy,
                    scope_if_creating,
                ),
                variables::ShellValueLiteral::Array(_) => {
                    Err(error::ErrorKind::AssigningListToArrayMember.into())
                }
            },
            None => env.update_or_add(name, value, updater, lookup_policy, scope_if_creating),
        }
    }
}

fn try_unset_in_map(
    map: &mut ShellVariableMap,
    name: &str,
) -> Result<Option<ShellVariable>, error::Error> {
    match map.get(name).map(|v| v.is_readonly()) {
        Some(true) => Err(error::ErrorKind::ReadonlyVariable.into()),
        Some(false) => Ok(map.unset(name)),
        None => Ok(None),
    }
}
