//! Variable lookup API.
//!
//! Entry points: `ShellEnvironment::lookup(name)` and `lookup_resolved(&resolved)`.
//! Each returns a builder:
//!   - Auto-resolving (`lookup`): `.get()` / `.try_get()` / `.bypassing_nameref()`
//!   - Pre-resolved / bypass: `.get()` / `.in_scope(policy)`

use std::borrow::Cow;

use super::names::parse_nameref_subscript;
use super::{EnvironmentLookup, EnvironmentScope, ResolvedName, ShellEnvironment};
use crate::{
    Shell, error, extensions,
    variables::{ShellValue, ShellValueUnsetType, ShellVariable},
};

/// Subscript-aware value extraction shared by [`ResolvedVarRef`] and
/// [`ResolvedVarRefMut`]. For non-subscripted variables, the returned `Cow`
/// borrows from the variable (zero-copy). For subscripted namerefs, an
/// allocation occurs.
fn resolve_value_str<'a, SE: extensions::ShellExtensions>(
    variable: &'a ShellVariable,
    nameref_subscript: Option<&str>,
    shell: &Shell<SE>,
) -> Option<Cow<'a, str>> {
    if matches!(variable.value(), ShellValue::Unset(_)) {
        return None;
    }
    if let Some(idx) = nameref_subscript {
        match variable.value().get_at(idx, shell) {
            Ok(Some(value)) => Some(Cow::Owned(value.into_owned())),
            _ => None,
        }
    } else {
        Some(variable.value().to_cow_str(shell))
    }
}

/// An immutable reference to a variable resolved through the nameref chain.
///
/// When a nameref resolves to a subscripted target like `arr[2]`, the variable
/// reference points to the **base** variable (`arr`) and [`has_subscript`](Self::has_subscript)
/// returns `true`. For value extraction, use [`value_str`](Self::value_str) —
/// `base_var().value().to_cow_str()` would return the whole array.
#[derive(Debug)]
pub struct ResolvedVarRef<'a> {
    scope: EnvironmentScope,
    variable: &'a ShellVariable,
    nameref_subscript: Option<String>,
}

impl<'a> ResolvedVarRef<'a> {
    pub(super) const fn new(
        scope: EnvironmentScope,
        variable: &'a ShellVariable,
        nameref_subscript: Option<String>,
    ) -> Self {
        Self {
            scope,
            variable,
            nameref_subscript,
        }
    }

    /// The scope in which the resolved variable was found.
    pub const fn scope(&self) -> EnvironmentScope {
        self.scope
    }

    /// The base variable — for type/attribute inspection. For subscripted
    /// namerefs (`ref → arr[2]`), this is the array `arr`, not element `arr[2]`.
    pub const fn base_var(&self) -> &'a ShellVariable {
        self.variable
    }

    /// Subscript-aware value extraction. For subscripted namerefs, returns
    /// the targeted element, not the whole array.
    pub fn value_str<SE: extensions::ShellExtensions>(
        &self,
        shell: &Shell<SE>,
    ) -> Option<Cow<'a, str>> {
        resolve_value_str(self.variable, self.nameref_subscript.as_deref(), shell)
    }

    /// Returns the resolved [`ShellValue`], handling subscripted namerefs.
    ///
    /// For non-subscripted variables, returns the value by reference. For
    /// subscripted namerefs (`ref → arr[2]`), returns an owned
    /// `ShellValue::String` containing the element value (or `Unset` if the
    /// element is missing).
    pub fn resolved_value<SE: extensions::ShellExtensions>(
        &self,
        shell: &Shell<SE>,
    ) -> Cow<'a, ShellValue> {
        if let Some(idx) = &self.nameref_subscript {
            match self.variable.value().get_at(idx, shell) {
                Ok(Some(value)) => Cow::Owned(ShellValue::String(value.into_owned())),
                _ => Cow::Owned(ShellValue::Unset(ShellValueUnsetType::Untyped)),
            }
        } else {
            Cow::Borrowed(self.variable.value())
        }
    }

    /// Whether the nameref resolved to a subscripted target (e.g., `arr[2]`).
    pub const fn has_subscript(&self) -> bool {
        self.nameref_subscript.is_some()
    }

    /// The subscript (if any) embedded in the resolved nameref target.
    pub fn subscript(&self) -> Option<&str> {
        self.nameref_subscript.as_deref()
    }
}

/// A mutable reference to a variable resolved through the nameref chain.
/// See [`ResolvedVarRef`] for subscript semantics.
#[derive(Debug)]
pub struct ResolvedVarRefMut<'a> {
    scope: EnvironmentScope,
    variable: &'a mut ShellVariable,
    nameref_subscript: Option<String>,
}

impl<'a> ResolvedVarRefMut<'a> {
    pub(super) const fn new(
        scope: EnvironmentScope,
        variable: &'a mut ShellVariable,
        nameref_subscript: Option<String>,
    ) -> Self {
        Self {
            scope,
            variable,
            nameref_subscript,
        }
    }

    /// The scope in which the resolved variable was found.
    pub const fn scope(&self) -> EnvironmentScope {
        self.scope
    }

    /// The base variable (immutable) — for type/attribute inspection.
    pub const fn base_var(&self) -> &ShellVariable {
        self.variable
    }

    /// The base variable (mutable) — for attribute mutation.
    ///
    /// `pub(crate)` because for subscripted namerefs (`ref → arr[2]`), this
    /// returns the *array*, not the element. Writing through it would
    /// overwrite the whole array. External callers should go through
    /// [`ShellEnvironment::update_or_add`] or
    /// [`ShellEnvironment::update_or_add_array_element`] which handle
    /// subscripts correctly.
    pub(crate) const fn base_var_mut(&mut self) -> &mut ShellVariable {
        self.variable
    }

    /// Subscript-aware value extraction. See [`ResolvedVarRef::value_str`].
    pub fn value_str<SE: extensions::ShellExtensions>(
        &self,
        shell: &Shell<SE>,
    ) -> Option<Cow<'_, str>> {
        resolve_value_str(self.variable, self.nameref_subscript.as_deref(), shell)
    }

    /// Whether the nameref resolved to a subscripted target (e.g., `arr[2]`).
    pub const fn has_subscript(&self) -> bool {
        self.nameref_subscript.is_some()
    }
}

/// Immutable lookup builder for auto-resolving nameref lookups.
pub struct VarLookup<'a> {
    pub(super) env: &'a ShellEnvironment,
    pub(super) name: &'a str,
}

impl<'a> VarLookup<'a> {
    /// Execute the lookup, resolving namerefs transparently. Silently returns
    /// `None` on circular-nameref errors; use [`try_get`](Self::try_get) to
    /// propagate them.
    pub fn get(self) -> Option<ResolvedVarRef<'a>> {
        self.env.get(self.name)
    }

    /// Like [`get`](Self::get), but propagates nameref resolution errors.
    pub fn try_get(self) -> Result<Option<ResolvedVarRef<'a>>, error::Error> {
        self.env.try_get(self.name)
    }

    /// Switch to bypass mode: look up the variable by its literal name
    /// without following nameref chains.
    #[must_use]
    pub const fn bypassing_nameref(self) -> DirectVarLookup<'a> {
        DirectVarLookup {
            env: self.env,
            name: self.name,
            policy: EnvironmentLookup::Anywhere,
        }
    }
}

/// Immutable lookup builder for pre-resolved or bypassed names.
/// Performs exact-name lookups without further nameref resolution.
pub struct DirectVarLookup<'a> {
    pub(super) env: &'a ShellEnvironment,
    pub(super) name: &'a str,
    pub(super) policy: EnvironmentLookup,
}

impl<'a> DirectVarLookup<'a> {
    /// Restrict the lookup to a specific scope.
    #[must_use]
    pub const fn in_scope(mut self, policy: EnvironmentLookup) -> Self {
        self.policy = policy;
        self
    }

    /// Execute the lookup without nameref resolution.
    pub fn get(self) -> Option<(EnvironmentScope, &'a ShellVariable)> {
        self.env
            .get_by_exact_name_using_policy(self.name, self.policy)
    }
}

/// Mutable lookup builder for auto-resolving nameref lookups.
pub struct VarLookupMut<'a> {
    pub(super) env: &'a mut ShellEnvironment,
    pub(super) name: &'a str,
}

impl<'a> VarLookupMut<'a> {
    /// Execute the lookup, resolving namerefs transparently. Silently returns
    /// `None` on circular-nameref errors; use [`try_get`](Self::try_get) to
    /// propagate them.
    pub fn get(self) -> Option<ResolvedVarRefMut<'a>> {
        self.env.get_mut(self.name)
    }

    /// Like [`get`](Self::get), but propagates nameref resolution errors.
    pub fn try_get(self) -> Result<Option<ResolvedVarRefMut<'a>>, error::Error> {
        self.env.try_get_mut(self.name)
    }

    /// Switch to bypass mode: look up the variable by its literal name
    /// without following nameref chains.
    #[must_use]
    pub const fn bypassing_nameref(self) -> DirectVarLookupMut<'a> {
        DirectVarLookupMut {
            env: self.env,
            name: self.name,
            policy: EnvironmentLookup::Anywhere,
        }
    }
}

/// Mutable lookup builder for pre-resolved or bypassed names.
pub struct DirectVarLookupMut<'a> {
    pub(super) env: &'a mut ShellEnvironment,
    pub(super) name: &'a str,
    pub(super) policy: EnvironmentLookup,
}

impl<'a> DirectVarLookupMut<'a> {
    /// Restrict the lookup to a specific scope.
    #[must_use]
    pub const fn in_scope(mut self, policy: EnvironmentLookup) -> Self {
        self.policy = policy;
        self
    }

    /// Execute the lookup without nameref resolution.
    pub fn get(self) -> Option<(EnvironmentScope, &'a mut ShellVariable)> {
        self.env
            .get_mut_by_exact_name_using_policy(self.name, self.policy)
    }
}

//
// Lookup methods on ShellEnvironment.
//
// Entry points for the lookup builders, plus the resolution and scope-walk
// helpers that back them. Mutation methods live in mutation.rs;
// nameref chain resolution lives in mod.rs.
//

impl ShellEnvironment {
    /// Creates an immutable lookup builder for `name`, resolving namerefs by default.
    /// Chain `.bypassing_nameref()` to skip nameref resolution.
    pub const fn lookup<'a>(&'a self, name: &'a str) -> VarLookup<'a> {
        VarLookup { env: self, name }
    }

    /// Creates an immutable lookup builder for an already-resolved name. The
    /// environment performs no further resolution or subscript parsing.
    /// Chain `.in_scope(policy)` to restrict by scope.
    pub fn lookup_resolved<'a>(&'a self, resolved: &'a ResolvedName) -> DirectVarLookup<'a> {
        DirectVarLookup {
            env: self,
            name: resolved.name(),
            policy: EnvironmentLookup::Anywhere,
        }
    }

    /// Creates a mutable lookup builder for `name`, resolving namerefs by default.
    /// Chain `.bypassing_nameref()` to skip nameref resolution.
    pub const fn lookup_mut<'a>(&'a mut self, name: &'a str) -> VarLookupMut<'a> {
        VarLookupMut { env: self, name }
    }

    /// Creates a mutable lookup builder for an already-resolved name. No further
    /// resolution or subscript parsing. Chain `.in_scope(policy)` to restrict by scope.
    pub fn lookup_mut_resolved<'a>(
        &'a mut self,
        resolved: &'a ResolvedName,
    ) -> DirectVarLookupMut<'a> {
        DirectVarLookupMut {
            env: self,
            name: resolved.name(),
            policy: EnvironmentLookup::Anywhere,
        }
    }

    /// Internal: resolving lookup that silently returns None on cycle errors.
    /// External callers go through `lookup(name).get()`.
    pub(crate) fn get<S: AsRef<str>>(&self, name: S) -> Option<ResolvedVarRef<'_>> {
        self.try_get(name).ok().flatten()
    }

    /// Internal: resolving lookup that propagates cycle errors.
    /// External callers go through `lookup(name).try_get()`.
    pub(crate) fn try_get<S: AsRef<str>>(
        &self,
        name: S,
    ) -> Result<Option<ResolvedVarRef<'_>>, error::Error> {
        let name = name.as_ref();
        // Fast path: not a nameref, return directly with one scope-walk.
        let Some((scope, var)) = self.get_by_exact_name(name) else {
            return Ok(None);
        };
        if !var.is_treated_as_nameref() {
            return Ok(Some(ResolvedVarRef::new(scope, var, None)));
        }
        // Slow path: walk the chain, then re-lookup the resolved base.
        let resolved = self.resolve_nameref_chain(name)?;
        let (base, subscript) = parse_nameref_subscript(resolved.as_ref());
        let subscript_owned = subscript.map(|s| s.to_owned());
        Ok(self
            .get_by_exact_name(base)
            .map(|(scope, var)| ResolvedVarRef::new(scope, var, subscript_owned)))
    }

    /// Internal: resolving mutable lookup that silently returns None on cycle errors.
    /// External callers go through `lookup_mut(name).get()`.
    pub(crate) fn get_mut<S: AsRef<str>>(&mut self, name: S) -> Option<ResolvedVarRefMut<'_>> {
        self.try_get_mut(name).ok().flatten()
    }

    /// Internal: resolving mutable lookup that propagates cycle errors.
    /// External callers go through `lookup_mut(name).try_get()`.
    pub(crate) fn try_get_mut<S: AsRef<str>>(
        &mut self,
        name: S,
    ) -> Result<Option<ResolvedVarRefMut<'_>>, error::Error> {
        let name = name.as_ref();
        // Single immutable scan: find the variable's scope index and check
        // if nameref resolution is needed — one traversal instead of two.
        let mut found_scope_idx = None;
        let mut is_nameref = false;
        for (rev_idx, (_scope_type, map)) in self.scopes.iter().rev().enumerate() {
            if let Some(var) = map.get(name) {
                found_scope_idx = Some(self.scopes.len() - 1 - rev_idx);
                is_nameref = var.is_treated_as_nameref();
                break;
            }
        }

        if !is_nameref {
            // Fast path: direct index access (O(1)) instead of a second full scan.
            let Some(idx) = found_scope_idx else {
                return Ok(None);
            };
            let (scope_type, map) = &mut self.scopes[idx];
            return Ok(map
                .get_mut(name)
                .map(|var| ResolvedVarRefMut::new(*scope_type, var, None)));
        }
        // Slow path for namerefs.
        let resolved = self.resolve_nameref_chain(name)?.into_owned();
        let (base, subscript) = parse_nameref_subscript(&resolved);
        let subscript_owned = subscript.map(|s| s.to_owned());
        let base = base.to_owned();
        Ok(self
            .get_mut_by_exact_name(base)
            .map(|(scope, var)| ResolvedVarRefMut::new(scope, var, subscript_owned)))
    }

    /// Looks up a variable by exact string name without nameref resolution.
    ///
    /// The name is used as a literal `HashMap` key — no subscript parsing, no
    /// nameref following. For subscripted targets like `"arr[2]"`, this does a
    /// literal lookup for the key `"arr[2]"` which won't match any variable,
    /// correctly terminating nameref chain resolution.
    pub(super) fn get_by_exact_name<S: AsRef<str>>(
        &self,
        name: S,
    ) -> Option<(EnvironmentScope, &ShellVariable)> {
        for (scope_type, map) in self.scopes.iter().rev() {
            if let Some(var) = map.get(name.as_ref()) {
                return Some((*scope_type, var));
            }
        }
        None
    }

    /// Looks up a variable mutably by exact string name without nameref resolution.
    /// See [`get_by_exact_name`](Self::get_by_exact_name) for semantics.
    pub(super) fn get_mut_by_exact_name<S: AsRef<str>>(
        &mut self,
        name: S,
    ) -> Option<(EnvironmentScope, &mut ShellVariable)> {
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if let Some(var) = map.get_mut(name.as_ref()) {
                return Some((*scope_type, var));
            }
        }
        None
    }

    /// Looks up a variable by exact string name with lookup policy, no nameref resolution.
    pub(super) fn get_by_exact_name_using_policy<N: AsRef<str>>(
        &self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<(EnvironmentScope, &ShellVariable)> {
        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }
            if lookup_policy.admits(*scope_type, local_count)
                && let Some(var) = var_map.get(name.as_ref())
            {
                return Some((*scope_type, var));
            }
            if lookup_policy.terminates_after(*scope_type) {
                break;
            }
        }
        None
    }

    /// Looks up a variable mutably by exact string name with lookup policy, no nameref resolution.
    pub(super) fn get_mut_by_exact_name_using_policy<N: AsRef<str>>(
        &mut self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<(EnvironmentScope, &mut ShellVariable)> {
        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter_mut().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }
            if lookup_policy.admits(*scope_type, local_count)
                && let Some(var) = var_map.get_mut(name.as_ref())
            {
                return Some((*scope_type, var));
            }
            if lookup_policy.terminates_after(*scope_type) {
                break;
            }
        }
        None
    }

    /// Retrieves the string value of a variable, resolving namerefs and subscripts
    /// correctly. Convenience for `self.get(name)?.value_str(shell)`.
    pub fn get_str<S: AsRef<str>, SE: extensions::ShellExtensions>(
        &self,
        name: S,
        shell: &Shell<SE>,
    ) -> Option<Cow<'_, str>> {
        self.get(name)?.value_str(shell)
    }

    /// Checks if a variable of the given name is set, resolving namerefs.
    /// For subscripted namerefs, checks whether the specific element exists.
    pub fn is_set<S: AsRef<str>, SE: extensions::ShellExtensions>(
        &self,
        name: S,
        shell: &Shell<SE>,
    ) -> bool {
        self.get(name).is_some_and(|resolved| {
            let value = resolved.base_var().value();
            if !value.is_set() {
                return false;
            }
            if let Some(idx) = resolved.subscript() {
                value.has_element_at(idx, shell)
            } else {
                true
            }
        })
    }
}
