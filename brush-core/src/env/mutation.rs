//! Mutation methods on [`ShellEnvironment`]: `unset`, `update_or_add`,
//! `add`, `set_var`, etc. The lookup and resolution APIs live in `mod.rs` and
//! `lookup.rs`; this file is the write-side counterpart.

use super::{EnvironmentLookup, EnvironmentScope, NameRef, ShellEnvironment, ShellVariableMap};
use crate::error;
use crate::variables::{self, ShellValue, ShellValueUnsetType, ShellVariable};

impl ShellEnvironment {
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
        let resolved = match name.into() {
            NameRef::Resolve(s) => match self.resolve_nameref(&s) {
                Ok(r) => r,
                Err(e) if matches!(e.kind(), error::ErrorKind::CircularNameReference(_)) => {
                    // Circular nameref: bash removes the variable itself.
                    return self.unset_raw(&s);
                }
                Err(e) => return Err(e),
            },
            NameRef::PreResolved(r) => r,
            NameRef::Bypass(s) => {
                super::names::assert_bare_name(&s, "NameRef::Bypass");
                return self.unset_raw(&s);
            }
        };
        if let Some(idx) = resolved.subscript() {
            if let Some((_, var)) = self.get_mut_by_exact_name(resolved.name()) {
                var.unset_index(idx)?;
            }
            return Ok(None);
        }
        self.unset_raw(resolved.name())
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

    /// Tries to unset an array element from the environment, using the given name and
    /// element index for lookup. Returns whether or not an element was unset.
    ///
    /// Resolves namerefs via [`get_mut`] to find the target variable; the explicit
    /// `index` parameter always takes precedence over any subscript embedded in a
    /// nameref target.
    pub fn unset_index(&mut self, name: &str, index: &str) -> Result<bool, error::Error> {
        if let Some(mut resolved) = self.get_mut(name) {
            resolved.base_var_mut().unset_index(index)
        } else {
            Ok(false)
        }
    }

    /// Update a variable in the environment, or add it if it doesn't already exist.
    ///
    /// Resolution strategy depends on the [`NameRef`] variant:
    /// - `Resolve` (default for `&str`) — follows nameref chains before writing.
    /// - `PreResolved` — uses the resolved base name; if a subscript is present,
    ///   redirects scalar values to array-element assignment.
    /// - `Bypass` — writes to the variable itself, bypassing namerefs.
    pub fn update_or_add(
        &mut self,
        name: impl Into<NameRef>,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let (base, subscript) = match name.into() {
            NameRef::Resolve(s) => {
                let resolved = self.resolve_nameref(&s)?;
                // Compound (array) assignment through a subscripted-target
                // nameref is rejected by bash as "not a valid identifier".
                // Scalar assignment redirects to the array element below.
                if let Some(idx) = &resolved.subscript {
                    if matches!(value, variables::ShellValueLiteral::Array(_)) {
                        return Err(error::ErrorKind::SubscriptedNameRefTarget {
                            name: resolved.name,
                            subscript: idx.clone(),
                        }
                        .into());
                    }
                }
                (resolved.name, resolved.subscript)
            }
            NameRef::PreResolved(r) => (r.name, r.subscript),
            NameRef::Bypass(s) => {
                super::names::assert_bare_name(&s, "NameRef::Bypass");
                (s, None)
            }
        };

        // If the resolved target includes an array subscript (e.g., arr[2]),
        // a scalar value writes to that element.
        if let Some(idx) = subscript {
            match value {
                variables::ShellValueLiteral::Scalar(scalar) => {
                    return self.update_or_add_array_element_impl(
                        base,
                        idx,
                        scalar,
                        updater,
                        lookup_policy,
                        scope_if_creating,
                    );
                }
                variables::ShellValueLiteral::Array(_) => {
                    // Already rejected above for Resolve. PreResolved callers
                    // are pre-validated at their own call sites.
                    return Err(error::ErrorKind::SubscriptedNameRefTarget {
                        name: base,
                        subscript: idx,
                    }
                    .into());
                }
            }
        }

        self.update_or_add_impl(base, value, updater, lookup_policy, scope_if_creating)
    }

    /// Convenience: set a variable value with default options.
    /// Resolves namerefs, looks anywhere, creates in global scope.
    pub fn set_var(
        &mut self,
        name: impl Into<String>,
        value: variables::ShellValueLiteral,
    ) -> Result<(), error::Error> {
        self.update_or_add(
            name.into(),
            value,
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )
    }

    /// Convenience: set an array element with default options.
    pub fn set_var_element(
        &mut self,
        name: impl Into<String>,
        index: String,
        value: String,
    ) -> Result<(), error::Error> {
        self.update_or_add_array_element(
            name.into(),
            index,
            value,
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )
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
    /// Resolution strategy depends on the [`NameRef`] variant (same as [`update_or_add`]).
    /// The explicit `index` parameter always takes precedence over any subscript
    /// embedded in a nameref target.
    pub fn update_or_add_array_element(
        &mut self,
        name: impl Into<NameRef>,
        index: String,
        value: String,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let base = match name.into() {
            NameRef::Resolve(s) => {
                let resolved = self.resolve_nameref(&s)?;
                // Subscripted-target nameref + explicit index is rejected by
                // bash (`arr[N][explicit_idx]` isn't a valid identifier).
                if let Some(idx) = resolved.subscript {
                    return Err(error::ErrorKind::SubscriptedNameRefTarget {
                        name: resolved.name,
                        subscript: idx,
                    }
                    .into());
                }
                resolved.name
            }
            NameRef::PreResolved(r) => {
                // Callers using PreResolved must pre-validate; the subscripted
                // case is a foot-gun that would silently drop the resolved
                // subscript in favor of the explicit index.
                debug_assert!(
                    r.subscript.is_none(),
                    "update_or_add_array_element: subscripted PreResolved is ambiguous \
                     (resolved={:?}[{:?}], explicit index also passed)",
                    r.name,
                    r.subscript,
                );
                r.name
            }
            NameRef::Bypass(s) => {
                super::names::assert_bare_name(&s, "NameRef::Bypass");
                s
            }
        };

        self.update_or_add_array_element_impl(
            base,
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
        super::names::assert_bare_name(&name, "ShellEnvironment::add");

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
