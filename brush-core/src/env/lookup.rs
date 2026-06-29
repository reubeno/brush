//! Variable lookup API.
//!
//! Immutable entry points: `ShellEnvironment::lookup(name)` (auto-resolving) and
//! `lookup_resolved(&resolved)` (pre-resolved/bypass). Mutable access is
//! pre-resolved only: `lookup_mut_resolved(&resolved)`. Each returns a builder:
//!   - Auto-resolving (`lookup`): `.get()` / `.bypassing_nameref()`
//!   - Pre-resolved / bypass: `.get()` / `.in_scope(policy)`
//!
//! Mutation through a nameref is done by resolving once (immutably) and then
//! writing via the resolved name — see `update_or_add` / `lookup_mut_resolved`.

use std::borrow::Cow;

use super::names::parse_nameref_subscript;
use super::{EnvironmentLookup, EnvironmentScope, ResolvedName, ShellEnvironment};
use crate::{
    Shell, error, extensions,
    variables::{ShellValue, ShellValueUnsetType, ShellVariable},
};

/// Subscript-aware value extraction for [`ResolvedVarRef`]. For non-subscripted
/// variables, the returned `Cow` borrows from the variable (zero-copy). For
/// subscripted namerefs, an allocation occurs.
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

/// Immutable lookup builder for auto-resolving nameref lookups.
pub struct VarLookup<'a> {
    pub(super) env: &'a ShellEnvironment,
    pub(super) name: &'a str,
}

impl<'a> VarLookup<'a> {
    /// Execute the lookup, resolving namerefs transparently. Silently returns
    /// `None` on nameref faults (cycle / max-depth).
    pub fn get(self) -> Option<ResolvedVarRef<'a>> {
        self.env.get_resolved(self.name)
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
    ///
    /// **Requires** a subscript-free `resolved` (the base name only) — strip any
    /// subscript with [`ResolvedName::without_subscript`] first and apply it at
    /// the call site (the element is not looked up here). Debug-asserts this.
    pub fn lookup_resolved<'a>(&'a self, resolved: &'a ResolvedName) -> DirectVarLookup<'a> {
        debug_assert!(
            resolved.subscript().is_none(),
            "lookup_resolved ignores the subscript on {resolved:?}; \
             apply the subscript at the call site",
        );
        DirectVarLookup {
            env: self,
            name: resolved.name(),
            policy: resolved.default_lookup_policy(),
        }
    }

    /// Creates a mutable lookup builder for an already-resolved name. No further
    /// resolution or subscript parsing. Chain `.in_scope(policy)` to restrict by
    /// scope. Like [`lookup_resolved`](Self::lookup_resolved), **requires** a
    /// subscript-free `resolved` (strip it first; debug-asserted).
    pub fn lookup_mut_resolved<'a>(
        &'a mut self,
        resolved: &'a ResolvedName,
    ) -> DirectVarLookupMut<'a> {
        debug_assert!(
            resolved.subscript().is_none(),
            "lookup_mut_resolved ignores the subscript on {resolved:?}; \
             apply the subscript at the call site",
        );
        DirectVarLookupMut {
            env: self,
            name: resolved.name(),
            policy: resolved.default_lookup_policy(),
        }
    }

    /// Internal: resolving lookup that silently returns None on nameref faults.
    /// External callers go through `lookup(name).get()`.
    pub(crate) fn get_resolved<S: AsRef<str>>(&self, name: S) -> Option<ResolvedVarRef<'_>> {
        self.try_get(name).ok().flatten()
    }

    /// Tries to retrieve an immutable reference by exact name without nameref
    /// resolution.
    pub fn get_exact<S: AsRef<str>>(&self, name: S) -> Option<(EnvironmentScope, &ShellVariable)> {
        self.get_by_exact_name(name)
    }

    /// Tries to retrieve an immutable reference to the variable with the given
    /// literal name in the environment.
    ///
    /// Deprecated compatibility wrapper for the pre-nameref API. This performs
    /// an exact lookup and does not resolve namerefs; use
    /// [`lookup`](Self::lookup) for nameref-aware lookups, or
    /// [`lookup(name).bypassing_nameref()`](Self::lookup) when exact lookup is
    /// intentional.
    #[deprecated(
        since = "0.5.0",
        note = "use ShellEnvironment::lookup(name).get() for nameref-aware lookup or lookup(name).bypassing_nameref().get() for exact lookup"
    )]
    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<(EnvironmentScope, &ShellVariable)> {
        self.get_exact(name)
    }

    /// Tries to retrieve a mutable reference by exact name without nameref
    /// resolution.
    pub fn get_mut_exact<S: AsRef<str>>(
        &mut self,
        name: S,
    ) -> Option<(EnvironmentScope, &mut ShellVariable)> {
        self.get_mut_by_exact_name(name)
    }

    /// Tries to retrieve a mutable reference to the variable with the given
    /// literal name in the environment.
    ///
    /// Deprecated compatibility wrapper for the pre-nameref API. This performs
    /// an exact lookup and does not resolve namerefs; new mutation paths should
    /// resolve once and use [`lookup_mut_resolved`](Self::lookup_mut_resolved).
    #[deprecated(
        since = "0.5.0",
        note = "use lookup_mut_resolved for pre-resolved names or the mutation helpers for nameref-aware writes"
    )]
    pub fn get_mut<S: AsRef<str>>(
        &mut self,
        name: S,
    ) -> Option<(EnvironmentScope, &mut ShellVariable)> {
        self.get_mut_exact(name)
    }

    /// Internal helper backing [`get`](Self::get): like `get`, but propagates
    /// nameref faults (cycle / max-depth) as an error instead of swallowing them.
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
        // Slow path: walk the chain, then re-lookup the resolved base under the
        // resolved scope. A self-referential nameref (`local -n x=x`) resolves
        // to the global `x`, so the re-lookup is global-scoped (not the usual
        // scope walk, which would find the local nameref again).
        let (resolved, scope) = self.resolve_nameref_chain(name)?;
        let (base, subscript) = parse_nameref_subscript(resolved.as_ref());
        let mut base = base.to_owned();
        let mut lookup_policy = scope.lookup_policy_or(EnvironmentLookup::Anywhere);

        // Bash has an rvalue-only special case: if `a` resolves to `b[1]` and
        // `b` is itself a nameref, reading `$a` reads element 1 of `b`'s target.
        // Writing through `a` still mutates `b[1]` itself, so this belongs only
        // in the immutable lookup path.
        if subscript.is_some()
            && self
                .get_by_exact_name_using_policy(&base, lookup_policy)
                .is_some_and(|(_, var)| var.is_treated_as_nameref())
        {
            let (base_resolved, base_scope) = self.resolve_nameref_chain(&base)?;
            let base_resolved = base_resolved.into_owned();
            let (resolved_base, base_subscript) = parse_nameref_subscript(&base_resolved);
            if base_subscript.is_some() {
                return Ok(None);
            }
            resolved_base.clone_into(&mut base);
            lookup_policy = base_scope.lookup_policy_or(EnvironmentLookup::Anywhere);
        }

        let subscript_owned = subscript.map(|s| s.to_owned());
        let found = self.get_by_exact_name_using_policy(&base, lookup_policy);
        Ok(found.map(|(scope, var)| ResolvedVarRef::new(scope, var, subscript_owned)))
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
    ///
    /// Internal: an environment method that needs the whole `Shell` is a
    /// layering inversion; the public surface is [`Shell::env_str`].
    pub(crate) fn get_resolved_str<S: AsRef<str>, SE: extensions::ShellExtensions>(
        &self,
        name: S,
        shell: &Shell<SE>,
    ) -> Option<Cow<'_, str>> {
        self.get_resolved(name)?.value_str(shell)
    }

    /// Tries to retrieve the string value of the variable with the given literal
    /// name in the environment.
    ///
    /// Deprecated compatibility wrapper for the pre-nameref API. This performs
    /// an exact lookup and returns the base variable value; use
    /// [`Shell::env_str`](crate::Shell::env_str) for nameref-aware string lookup.
    #[deprecated(
        since = "0.5.0",
        note = "use Shell::env_str for nameref-aware string lookup"
    )]
    pub fn get_str<S: AsRef<str>, SE: extensions::ShellExtensions>(
        &self,
        name: S,
        shell: &Shell<SE>,
    ) -> Option<Cow<'_, str>> {
        self.get_by_exact_name(name)
            .map(|(_, v)| v.value().to_cow_str(shell))
    }

    /// Checks if a variable of the given name is set, resolving namerefs.
    /// For subscripted namerefs, checks whether the specific element exists.
    ///
    /// Internal: the public surface is [`Shell::env_is_set`].
    pub(crate) fn is_resolved_set<S: AsRef<str>, SE: extensions::ShellExtensions>(
        &self,
        name: S,
        shell: &Shell<SE>,
    ) -> bool {
        self.get_resolved(name).is_some_and(|resolved| {
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

    /// Checks if a variable of the given literal name is set in the environment.
    ///
    /// Deprecated compatibility wrapper for the pre-nameref API. This performs
    /// an exact lookup and does not resolve namerefs; use
    /// [`Shell::env_is_set`](crate::Shell::env_is_set) for nameref-aware checks.
    #[deprecated(
        since = "0.5.0",
        note = "use Shell::env_is_set for nameref-aware checks"
    )]
    pub fn is_set<S: AsRef<str>>(&self, name: S) -> bool {
        self.get_by_exact_name(name)
            .is_some_and(|(_, var)| var.value().is_set())
    }

    /// Tries to retrieve an immutable reference to a variable from the
    /// environment, using the given literal name and lookup policy.
    ///
    /// Deprecated compatibility wrapper for the pre-nameref API. This performs
    /// exact lookup and does not resolve namerefs; use [`lookup_resolved`] or
    /// [`lookup(name).bypassing_nameref()`](Self::lookup) with `.in_scope(...)`.
    #[deprecated(
        since = "0.5.0",
        note = "use lookup_resolved(...).in_scope(policy).get() or lookup(name).bypassing_nameref().in_scope(policy).get()"
    )]
    pub fn get_using_policy<N: AsRef<str>>(
        &self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<&ShellVariable> {
        self.get_by_exact_name_using_policy(name, lookup_policy)
            .map(|(_, var)| var)
    }

    /// Tries to retrieve a mutable reference to a variable from the environment,
    /// using the given literal name and lookup policy.
    ///
    /// Deprecated compatibility wrapper for the pre-nameref API. This performs
    /// exact lookup and does not resolve namerefs.
    #[deprecated(
        since = "0.5.0",
        note = "use lookup_mut_resolved(...).in_scope(policy).get() for pre-resolved names"
    )]
    pub fn get_mut_using_policy<N: AsRef<str>>(
        &mut self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<&mut ShellVariable> {
        self.get_mut_by_exact_name_using_policy(name, lookup_policy)
            .map(|(_, var)| var)
    }
}
