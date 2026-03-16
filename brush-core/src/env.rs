//! Implements a shell variable environment.

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map;

use crate::Shell;
use crate::error;
use crate::extensions;
use crate::variables::{self, ShellValue, ShellValueUnsetType, ShellVariable};

/// Maximum depth for nameref chain resolution. Matches bash 5.2's internal
/// `NAMEREF_MAX` limit (8). Prevents infinite loops on pathological chains
/// and guards against stack-like resource exhaustion.
const MAX_NAMEREF_DEPTH: usize = 8;

// ─── Variable lookup API design ─────────────────────────────────────
//
// Two entry points for lookups: `lookup()` and `lookup_mut()`.
// Both accept `&str` or `&ResolvedName`, dispatching via the
// `IntoVarLookup` / `IntoVarLookupMut` traits:
//
//   env.lookup("name").get()                     // auto-resolve → ResolvedVarRef
//   env.lookup("name").bypassing_nameref().get() // bypass → (Scope, &Var)
//   env.lookup(&resolved).get()                  // pre-resolved → (Scope, &Var)
//   env.lookup(&resolved).in_scope(policy).get() // pre-resolved + scoped
//   env.lookup_mut("name").get()                 // auto-resolve → ResolvedVarRefMut
//   env.lookup_mut("name").bypassing_nameref().get() // bypass → (Scope, &mut Var)
//   env.lookup_mut(&resolved).get()              // pre-resolved → (Scope, &mut Var)
//
// Lookup key types:
//   - `&str` — auto-resolves namerefs, returns `ResolvedVarRef` / `ResolvedVarRefMut`.
//     Chain `.bypassing_nameref()` to skip resolution and inspect the variable
//     itself (e.g., checking `-R`, `unset -n`).
//   - `&ResolvedName` — name already resolved through the nameref chain
//     (constructed by `resolve_nameref()`). No further resolution.
//
// Mutation strategies:
//   - `update_or_add()` — resolves namerefs transparently before writing.
//   - `update_or_add_bypassing_nameref()` — skips resolution (for `for-in`
//     loop variable assignment, which retargets the nameref itself).
//   - `unset()` / `unset_bypassing_nameref()` — analogous pair for removal.
//
// Auto-resolving lookups return wrapper structs (`ResolvedVarRef` /
// `ResolvedVarRefMut`) that guide callers toward correct usage:
//   - `base_var()` / `base_var_mut()` — for attribute/type inspection.
//     Named "base" to remind callers that for subscripted namerefs
//     (e.g., `ref → arr[2]`), this is the array, not the element.
//   - `value_str(shell)` — for subscript-aware value extraction.
// ────────────────────────────────────────────────────────────────────

/// A fully resolved nameref target, split into base name and optional array subscript.
///
/// When a nameref resolves to a plain variable name like `"target"`, `subscript` is `None`.
/// When it resolves to an array element like `"arr[2]"`, `name` is `"arr"` and `subscript`
/// is `Some("2")`.
///
/// Constructed by [`ShellEnvironment::resolve_nameref`]. To look up a variable
/// without nameref resolution, use `env.lookup("name").bypassing_nameref().get()`.
#[derive(Clone, Debug)]
pub struct ResolvedName {
    name: String,
    subscript: Option<String>,
}

impl ResolvedName {
    /// The base variable name (after nameref resolution and subscript extraction).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The array subscript, if the resolved target includes one (e.g., `arr[2]` yields `Some("2")`).
    pub fn subscript(&self) -> Option<&str> {
        self.subscript.as_deref()
    }

    /// Consumes this `ResolvedName` and returns the base variable name.
    pub fn into_name(self) -> String {
        self.name
    }

    /// Returns a copy with the subscript stripped, keeping only the base name.
    ///
    /// Useful when a nameref resolved to a subscripted target (e.g., `arr[2]`)
    /// but you need to operate on the base variable (e.g., for attribute changes
    /// or whole-array expansion).
    #[must_use]
    pub fn without_subscript(&self) -> Self {
        Self {
            name: self.name.clone(),
            subscript: None,
        }
    }

    /// Parse a resolved nameref target string into base name and optional subscript.
    fn parse(resolved: String) -> Self {
        let (base, sub) = parse_nameref_subscript(&resolved);
        if let Some(idx) = sub {
            Self {
                name: base.to_owned(),
                subscript: Some(idx.to_owned()),
            }
        } else {
            Self {
                name: resolved,
                subscript: None,
            }
        }
    }

    /// Creates a `ResolvedName` wrapping a name that the caller asserts has
    /// **already been resolved** through the nameref chain (e.g., via
    /// [`ShellEnvironment::resolve_nameref`] or [`ShellEnvironment::resolve_nameref_to_name`]).
    ///
    /// # When to use
    ///
    /// Use this when you've resolved a name externally and need to pass the
    /// result to `lookup()` / `lookup_mut()` without re-resolving.
    ///
    /// # When NOT to use
    ///
    /// Do NOT use this to skip nameref resolution. If you want to inspect the
    /// variable itself (e.g., checking `[[ -R ref ]]` or `declare -p`), use
    /// `env.lookup("name").bypassing_nameref()` instead.
    pub fn already_resolved(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            subscript: None,
        }
    }
}

/// Subscript-aware value extraction shared by [`ResolvedVarRef`] and
/// [`ResolvedVarRefMut`]. Correctly handles subscripted namerefs: if the
/// nameref resolved to `arr[2]`, returns the value of `arr[2]`, not the
/// whole array. For the non-subscript case, the returned `Cow` borrows from
/// the variable (zero-copy). For subscripted namerefs, an allocation occurs.
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
/// returns `true`.
///
/// - For **attribute/type inspection** (is it an array? exported? readonly?),
///   use [`base_var`](Self::base_var) — the base variable is always correct
///   for these queries, even for subscripted namerefs.
/// - For **value extraction**, use [`value_str`](Self::value_str) — it handles
///   subscripts correctly. Do NOT call `base_var().value().to_cow_str()` directly;
///   that would return the whole array instead of the targeted element.
#[derive(Debug)]
pub struct ResolvedVarRef<'a> {
    scope: EnvironmentScope,
    variable: &'a ShellVariable,
    nameref_subscript: Option<String>,
}

impl<'a> ResolvedVarRef<'a> {
    /// The scope in which the resolved variable was found.
    pub const fn scope(&self) -> EnvironmentScope {
        self.scope
    }

    /// The base variable — for type/attribute inspection.
    ///
    /// Named `base_var` (not `var`) as a reminder: for subscripted namerefs
    /// (`ref → arr[2]`), this returns the array `arr`, not element `arr[2]`.
    /// For value extraction, use [`value_str`](Self::value_str) instead.
    ///
    /// The returned reference has the environment's lifetime (`'a`), so it
    /// remains valid even after the `ResolvedVarRef` is dropped.
    pub const fn base_var(&self) -> &'a ShellVariable {
        self.variable
    }

    /// Subscript-aware value extraction.
    ///
    /// Correctly handles subscripted namerefs: if the nameref resolved to
    /// `arr[2]`, this returns the value of `arr[2]`, not the whole array.
    /// This is the safe way to get a string value through a resolved reference.
    pub fn value_str<SE: extensions::ShellExtensions>(
        &self,
        shell: &Shell<SE>,
    ) -> Option<Cow<'a, str>> {
        resolve_value_str(self.variable, self.nameref_subscript.as_deref(), shell)
    }

    /// Returns the resolved [`ShellValue`], correctly handling subscripted
    /// namerefs.
    ///
    /// For non-subscripted variables, returns the value directly (by reference).
    /// For subscripted namerefs (`ref → arr[2]`), returns an owned
    /// `ShellValue::String` containing the element value. If the element is
    /// unset, returns `ShellValue::Unset`.
    ///
    /// Use this when you need to pattern-match on the value type (e.g., to
    /// distinguish `String` from `IndexedArray`). For simple string extraction,
    /// prefer [`value_str`](Self::value_str).
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
}

/// A mutable reference to a variable resolved through the nameref chain.
///
/// See [`ResolvedVarRef`] for subscript semantics.
///
/// - For **reading** the current value, use [`value_str`](Self::value_str).
/// - For **attribute mutation** (export, readonly, etc.), use
///   [`base_var_mut`](Self::base_var_mut).
/// - For **type inspection**, use [`base_var`](Self::base_var) — the base
///   variable is always correct for type/attribute queries.
#[derive(Debug)]
pub struct ResolvedVarRefMut<'a> {
    scope: EnvironmentScope,
    variable: &'a mut ShellVariable,
    nameref_subscript: Option<String>,
}

impl ResolvedVarRefMut<'_> {
    /// The scope in which the resolved variable was found.
    pub const fn scope(&self) -> EnvironmentScope {
        self.scope
    }

    /// The base variable (immutable) — for type/attribute inspection.
    ///
    /// See [`ResolvedVarRef::base_var`] for details. For value extraction,
    /// use [`value_str`](Self::value_str) instead.
    pub const fn base_var(&self) -> &ShellVariable {
        self.variable
    }

    /// The base variable (mutable) — for attribute mutation (export, readonly, etc.).
    ///
    /// Named `base_var_mut` as a reminder: for subscripted namerefs
    /// (`ref → arr[2]`), this returns the array `arr`, not element `arr[2]`.
    ///
    /// # Write-through limitation
    ///
    /// This method provides no safe path for writing to a subscripted nameref
    /// element. Calling `base_var_mut().assign(val, false)` on a subscripted
    /// nameref would overwrite the **entire array**, not just the targeted
    /// element. To write through a subscripted nameref, use
    /// [`ShellEnvironment::update_or_add`] or
    /// [`ShellEnvironment::update_or_add_array_element`] instead — those
    /// methods handle subscript extraction from the resolved nameref target.
    pub const fn base_var_mut(&mut self) -> &mut ShellVariable {
        self.variable
    }

    /// Subscript-aware value extraction.
    ///
    /// See [`ResolvedVarRef::value_str`] for details.
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

// ─── Lookup builder API ──────────────────────────────────────────────
//
// Entry points: `lookup()` and `lookup_mut()`, accepting `&str` or
// `&ResolvedName` via the `IntoVarLookup` / `IntoVarLookupMut` traits.
//
// Usage:
//   env.lookup("name").get()                          // auto-resolve → ResolvedVarRef
//   env.lookup("name").bypassing_nameref().get()      // bypass → (Scope, &Var)
//   env.lookup(&resolved).get()                       // pre-resolved → (Scope, &Var)
//   env.lookup(&resolved).in_scope(policy).get()      // pre-resolved + scoped
//   env.lookup_mut("name").get()                      // auto-resolve → ResolvedVarRefMut
//   env.lookup_mut("name").bypassing_nameref().get()  // bypass → (Scope, &mut Var)
//   env.lookup_mut(&resolved).get()                   // pre-resolved → (Scope, &mut Var)
// ────────────────────────────────────────────────────────────────────

/// Immutable lookup builder for auto-resolving nameref lookups.
pub struct VarLookup<'a> {
    env: &'a ShellEnvironment,
    name: &'a str,
}

impl<'a> VarLookup<'a> {
    /// Execute the lookup, resolving namerefs transparently.
    pub fn get(self) -> Option<ResolvedVarRef<'a>> {
        self.env.get(self.name)
    }

    /// Switch to bypass mode: look up the variable by its literal name
    /// without following nameref chains. Use this when you need to inspect
    /// or operate on a variable itself — e.g., checking if it IS a nameref
    /// (`[[ -R ref ]]`), displaying its attributes (`declare -p`), or
    /// checking existence in a specific scope.
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
    env: &'a ShellEnvironment,
    name: &'a str,
    policy: EnvironmentLookup,
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
    env: &'a mut ShellEnvironment,
    name: &'a str,
}

impl<'a> VarLookupMut<'a> {
    /// Execute the lookup, resolving namerefs transparently.
    pub fn get(self) -> Option<ResolvedVarRefMut<'a>> {
        self.env.get_mut(self.name)
    }

    /// Switch to bypass mode: look up the variable by its literal name
    /// without following nameref chains. See [`VarLookup::bypassing_nameref`]
    /// for when to use this.
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
/// Performs exact-name lookups without further nameref resolution.
pub struct DirectVarLookupMut<'a> {
    env: &'a mut ShellEnvironment,
    name: &'a str,
    policy: EnvironmentLookup,
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

/// Trait for types that can be used as lookup keys with [`ShellEnvironment::lookup`].
///
/// Implemented for `&str` (auto-resolving nameref lookups) and `&ResolvedName`
/// (pre-resolved lookups).
pub trait IntoVarLookup<'a> {
    /// The builder type returned by `lookup()`.
    type Lookup;
    /// Convert this name into an immutable lookup builder.
    fn into_lookup(self, env: &'a ShellEnvironment) -> Self::Lookup;
}

impl<'a> IntoVarLookup<'a> for &'a str {
    type Lookup = VarLookup<'a>;
    fn into_lookup(self, env: &'a ShellEnvironment) -> VarLookup<'a> {
        VarLookup { env, name: self }
    }
}

impl<'a> IntoVarLookup<'a> for &'a ResolvedName {
    type Lookup = DirectVarLookup<'a>;
    fn into_lookup(self, env: &'a ShellEnvironment) -> DirectVarLookup<'a> {
        DirectVarLookup {
            env,
            name: self.name(),
            policy: EnvironmentLookup::Anywhere,
        }
    }
}

/// Trait for types that can be used as lookup keys with [`ShellEnvironment::lookup_mut`].
///
/// Implemented for `&str` (auto-resolving nameref lookups) and `&ResolvedName`
/// (pre-resolved lookups).
pub trait IntoVarLookupMut<'a> {
    /// The builder type returned by `lookup_mut()`.
    type Lookup;
    /// Convert this name into a mutable lookup builder.
    fn into_lookup_mut(self, env: &'a mut ShellEnvironment) -> Self::Lookup;
}

impl<'a> IntoVarLookupMut<'a> for &'a str {
    type Lookup = VarLookupMut<'a>;
    fn into_lookup_mut(self, env: &'a mut ShellEnvironment) -> VarLookupMut<'a> {
        VarLookupMut { env, name: self }
    }
}

impl<'a> IntoVarLookupMut<'a> for &'a ResolvedName {
    type Lookup = DirectVarLookupMut<'a>;
    fn into_lookup_mut(self, env: &'a mut ShellEnvironment) -> DirectVarLookupMut<'a> {
        DirectVarLookupMut {
            env,
            name: self.name(),
            policy: EnvironmentLookup::Anywhere,
        }
    }
}

/// Represents the policy for looking up variables in a shell environment.
#[derive(Clone, Copy)]
pub enum EnvironmentLookup {
    /// Look anywhere.
    Anywhere,
    /// Look only in the global scope.
    OnlyInGlobal,
    /// Look only in the current local scope.    
    OnlyInCurrentLocal,
    /// Look only in local scopes.
    OnlyInLocal,
}

/// Represents a shell environment scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EnvironmentScope {
    /// Scope local to a function instance
    Local,
    /// Globals
    Global,
    /// Transient overrides for a command invocation
    Command,
}

impl std::fmt::Display for EnvironmentScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Global => write!(f, "global"),
            Self::Command => write!(f, "command"),
        }
    }
}

/// A guard that pushes a scope onto a shell environment and pops it when dropped.
pub(crate) struct ScopeGuard<'a, SE: extensions::ShellExtensions> {
    scope_type: EnvironmentScope,
    shell: &'a mut crate::Shell<SE>,
    detached: bool,
}

impl<'a, SE: extensions::ShellExtensions> ScopeGuard<'a, SE> {
    /// Creates a new scope guard, pushing the given scope type onto the environment.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell whose environment to modify.
    /// * `scope_type` - The type of scope to push.
    pub fn new(shell: &'a mut crate::Shell<SE>, scope_type: EnvironmentScope) -> Self {
        shell.env_mut().push_scope(scope_type);
        Self {
            scope_type,
            shell,
            detached: false,
        }
    }

    /// Returns a mutable reference to the shell.
    pub const fn shell(&mut self) -> &mut crate::Shell<SE> {
        self.shell
    }

    /// Detaches the guard, preventing it from popping the scope on drop.
    pub const fn detach(&mut self) {
        self.detached = true;
    }
}

impl<SE: extensions::ShellExtensions> Drop for ScopeGuard<'_, SE> {
    fn drop(&mut self) {
        if !self.detached {
            let _ = self.shell.env_mut().pop_scope(self.scope_type);
        }
    }
}

/// Represents the shell variable environment, composed of a stack of scopes.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ShellEnvironment {
    /// Stack of scopes, with the top of the stack being the current scope.
    scopes: Vec<(EnvironmentScope, ShellVariableMap)>,
    /// Whether or not to auto-export variables on creation or modification.
    export_variables_on_modification: bool,
    /// Count of total entries (may include duplicates with shadowed variables).
    entry_count: usize,
}

impl Default for ShellEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellEnvironment {
    /// Returns a new shell environment.
    pub fn new() -> Self {
        Self {
            scopes: vec![(EnvironmentScope::Global, ShellVariableMap::default())],
            export_variables_on_modification: false,
            entry_count: 0,
        }
    }

    /// Pushes a new scope of the given type onto the environment's scope stack.
    ///
    /// # Arguments
    ///
    /// * `scope_type` - The type of scope to push.
    pub fn push_scope(&mut self, scope_type: EnvironmentScope) {
        self.scopes.push((scope_type, ShellVariableMap::default()));
    }

    /// Pops the top-most scope off the environment's scope stack.
    ///
    /// # Arguments
    ///
    /// * `expected_scope_type` - The type of scope that is expected to be atop the stack.
    pub fn pop_scope(&mut self, expected_scope_type: EnvironmentScope) -> Result<(), error::Error> {
        // TODO(env): Should we panic instead on failure? It's effectively a broken invariant.
        match self.scopes.pop() {
            Some((actual_scope_type, _)) if actual_scope_type == expected_scope_type => Ok(()),
            Some((actual_scope_type, _)) => Err(error::ErrorKind::UnexpectedScopeType {
                expected: expected_scope_type,
                actual: actual_scope_type,
            }
            .into()),
            None => Err(error::ErrorKind::MissingScope.into()),
        }
    }

    //
    // Nameref resolution
    //

    /// Resolves a nameref chain, returning the final target name string or an
    /// error on circular references.
    ///
    /// This is the lowest-level resolution API. Prefer [`resolve_nameref`] (which
    /// also parses array subscripts) or [`resolve_nameref_to_name`] (which returns
    /// just the name without subscript parsing) unless you need the raw `Cow<str>`.
    fn resolve_nameref_chain<'a>(
        &'a self,
        name: &'a str,
    ) -> Result<Cow<'a, str>, error::Error> {
        // Quick check: is this even a nameref?
        let first_target = match self.get_by_exact_name(name) {
            Some((_, var)) if var.is_treated_as_nameref() => match var.value() {
                ShellValue::String(s) if !s.is_empty() => s.as_str(),
                _ => return Ok(Cow::Borrowed(name)),
            },
            _ => return Ok(Cow::Borrowed(name)),
        };

        // Follow the chain with cycle detection and a hard depth limit.
        // All references borrow from `self` (immutable), so no allocations
        // are needed on the happy path. We use a Vec instead of a HashSet
        // because real-world nameref chains are short (1-3 levels); linear
        // scan over a small vec beats hashing.
        let mut current: &'a str = first_target;
        let mut visited: Vec<&'a str> = Vec::with_capacity(4);
        visited.push(name);

        loop {
            if visited.contains(&current) {
                return Err(
                    error::ErrorKind::CircularNameReference(current.to_owned()).into()
                );
            }

            // N.B. When `current` is a subscripted target like "arr[2]",
            // `get_by_exact_name` does a literal HashMap lookup for "arr[2]"
            // which won't match any variable — correctly terminating the chain.
            // The subscript is parsed later by `resolve_nameref` /
            // `parse_nameref_subscript`. Do NOT "fix" this to parse subscripts
            // here; that would cause double resolution when callers use
            // resolve_nameref().
            match self.get_by_exact_name(current) {
                Some((_, var)) if var.is_treated_as_nameref() => match var.value() {
                    ShellValue::String(s) if !s.is_empty() => {
                        visited.push(current);
                        // Check depth *after* following this link, matching bash's
                        // NAMEREF_MAX which counts resolution steps, not chain length.
                        if visited.len() > MAX_NAMEREF_DEPTH {
                            return Err(
                                error::ErrorKind::CircularNameReference(
                                    current.to_owned(),
                                )
                                .into(),
                            );
                        }
                        current = s.as_str();
                    }
                    _ => return Ok(Cow::Borrowed(current)),
                },
                _ => return Ok(Cow::Borrowed(current)),
            }
        }
    }

    /// Resolves a nameref chain and parses any array subscript from the target.
    /// Returns `Err` on circular references.
    ///
    /// This is the preferred high-level API for nameref resolution when callers need
    /// both the resolved base name and any subscript. For variable lookups that
    /// resolve namerefs and surface subscripts, use [`get`]/[`get_mut`] instead.
    pub fn resolve_nameref(&self, name: &str) -> Result<ResolvedName, error::Error> {
        let resolved = self.resolve_nameref_chain(name)?;
        // Fast path: if resolution returned the input name unchanged, skip the
        // allocation and subscript parse — the original name can't contain a
        // subscript (variable names can't contain `[`). We use string equality
        // rather than pointer equality because `resolve_nameref_chain` may
        // return `Cow::Borrowed` from either the input name (identity) or a
        // target variable's value (resolution) — only the identity case should
        // take this fast path. Self-references (e.g., nameref `x` pointing to
        // `"x"`) cannot reach here because cycle detection in
        // `resolve_nameref_chain` returns `Err` before producing a borrowed
        // value equal to the input.
        if resolved.as_ref() == name {
            return Ok(ResolvedName {
                name: name.to_owned(),
                subscript: None,
            });
        }
        Ok(ResolvedName::parse(resolved.into_owned()))
    }

    /// Resolves a nameref chain, returning only the final target name without
    /// parsing array subscripts.
    ///
    /// Use this when you need the resolved name as-is (e.g., for `[[ -v ref ]]`
    /// where bash treats the resolved target as a literal variable name and does
    /// NOT parse subscript syntax from it). In bash, `[[ -v ref ]]` where
    /// `ref → arr[2]` looks for a variable literally named `"arr[2]"`, not
    /// array element `arr` at index `2`.
    pub fn resolve_nameref_to_name(&self, name: &str) -> Result<String, error::Error> {
        let resolved = self.resolve_nameref_chain(name)?;
        Ok(resolved.into_owned())
    }

    // ─── Circular-nameref error handling policy ─────────────────────────
    //
    // Circular namerefs can be handled three ways depending on context:
    //
    // 1. **Warn + identity fallback** — used for value expansion (`${ref}`,
    //    `${!ref[@]}`, etc.) where bash emits a warning to stderr and treats the
    //    variable as unset. See `WordExpander::resolve_nameref_or_self()` in
    //    expansion.rs.
    //
    // 2. **Propagate the error** — used for declarations (`declare -x ref`)
    //    where bash fails the command. Callers use `resolve_nameref()?` directly.
    //
    // 3. **Silent identity fallback** — used for tests (`[[ -v ref ]]`) and
    //    array element unset (`unset ref[N]`) where bash silently treats the
    //    variable as not found. Use `resolve_nameref_or_default()` below.
    //
    // Builtins that emit their own warnings (e.g., `export`) handle the error
    // inline because they format the warning with `context.command_name`.
    // ────────────────────────────────────────────────────────────────────

    /// Resolves a nameref, silently falling back to an identity `ResolvedName`
    /// on errors (circular references, depth exhaustion).
    ///
    /// Use this in contexts where bash silently treats circular namerefs as
    /// not found — e.g., `[[ -v ref ]]`, `unset ref[N]`. For contexts that
    /// should emit a warning, handle the error at the callsite (expansion.rs
    /// uses `warn_nameref_error()`; builtins use `context.stderr()`).
    pub fn resolve_nameref_or_default(&self, name: &str) -> ResolvedName {
        self.resolve_nameref(name)
            .unwrap_or_else(|_| ResolvedName::already_resolved(name))
    }

    //
    // Iterators/Getters
    //

    /// Returns an iterator over all exported variables defined in the environment.
    ///
    /// Namerefs are included: in bash, an exported nameref is passed to child
    /// processes with its literal string value (i.e., the target variable's name,
    /// not the target's value). For example, `declare -nx ref=target` exports
    /// `ref=target` to children.
    pub fn iter_exported(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        // We won't actually need to store all entries, but we expect it should be
        // within the same order.
        let mut visible_vars: HashMap<&String, &ShellVariable> =
            HashMap::with_capacity(self.entry_count);

        for (_, var_map) in self.scopes.iter().rev() {
            for (name, var) in var_map.iter().filter(|(_, v)| v.is_exported()) {
                // Only insert the variable if it hasn't been seen yet.
                if let hash_map::Entry::Vacant(entry) = visible_vars.entry(name) {
                    entry.insert(var);
                }
            }
        }

        visible_vars.into_iter()
    }

    /// Returns an iterator over all the variables defined in the environment.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        self.iter_using_policy(EnvironmentLookup::Anywhere)
    }

    /// Returns an iterator over all the variables defined in the environment,
    /// using the given lookup policy.
    ///
    /// # Arguments
    ///
    /// * `lookup_policy` - The policy to use when looking up variables.
    pub fn iter_using_policy(
        &self,
        lookup_policy: EnvironmentLookup,
    ) -> impl Iterator<Item = (&String, &ShellVariable)> {
        // We won't actually need to store all entries, but we expect it should be
        // within the same order.
        let mut visible_vars: HashMap<&String, &ShellVariable> =
            HashMap::with_capacity(self.entry_count);

        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            match lookup_policy {
                EnvironmentLookup::Anywhere => (),
                EnvironmentLookup::OnlyInGlobal => {
                    if !matches!(scope_type, EnvironmentScope::Global) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInCurrentLocal => {
                    if !(matches!(scope_type, EnvironmentScope::Local) && local_count == 1) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInLocal => {
                    if !matches!(scope_type, EnvironmentScope::Local) {
                        continue;
                    }
                }
            }

            for (name, var) in var_map.iter() {
                // Only insert the variable if it hasn't been seen yet.
                if let hash_map::Entry::Vacant(entry) = visible_vars.entry(name) {
                    entry.insert(var);
                }
            }

            if matches!(scope_type, EnvironmentScope::Local)
                && matches!(lookup_policy, EnvironmentLookup::OnlyInCurrentLocal)
            {
                break;
            }
        }

        visible_vars.into_iter()
    }

    /// Creates an immutable lookup builder. Accepts `&str` (auto-resolving
    /// nameref lookups) or `&ResolvedName` (pre-resolved lookups with optional
    /// scope restriction via `.in_scope()`).
    ///
    /// For `&str` lookups, chain `.bypassing_nameref()` to skip resolution and
    /// inspect the variable itself.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// env.lookup("name").get()                          // → Option<ResolvedVarRef>
    /// env.lookup("name").bypassing_nameref().get()      // → Option<(Scope, &Var)>
    /// env.lookup(&resolved).get()                       // → Option<(Scope, &Var)>
    /// env.lookup(&resolved).in_scope(policy).get()      // → Option<(Scope, &Var)>
    /// ```
    pub fn lookup<'a, N: IntoVarLookup<'a>>(&'a self, name: N) -> N::Lookup {
        name.into_lookup(self)
    }

    /// Creates a mutable lookup builder. Accepts `&str` (auto-resolving
    /// nameref lookups) or `&ResolvedName` (pre-resolved lookups with optional
    /// scope restriction via `.in_scope()`).
    ///
    /// For `&str` lookups, chain `.bypassing_nameref()` to skip resolution.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// env.lookup_mut("name").get()                          // → Option<ResolvedVarRefMut>
    /// env.lookup_mut("name").bypassing_nameref().get()      // → Option<(Scope, &mut Var)>
    /// env.lookup_mut(&resolved).get()                       // → Option<(Scope, &mut Var)>
    /// env.lookup_mut(&resolved).in_scope(policy).get()      // → Option<(Scope, &mut Var)>
    /// ```
    pub fn lookup_mut<'a, N: IntoVarLookupMut<'a>>(&'a mut self, name: N) -> N::Lookup {
        name.into_lookup_mut(self)
    }

    /// Looks up a variable, resolving namerefs transparently.
    ///
    /// Returns a [`ResolvedVarRef`] that provides safe access to the variable:
    /// - [`base_var()`](ResolvedVarRef::base_var) for attribute/type inspection
    /// - [`value_str()`](ResolvedVarRef::value_str) for subscript-aware value extraction
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<ResolvedVarRef<'_>> {
        let name = name.as_ref();
        // Fast path: if the variable isn't a nameref, return it directly with
        // a single scope-stack traversal (avoids the double walk through
        // try_resolve_nameref_chain + get_raw for the common non-nameref case).
        let (scope, var) = self.get_by_exact_name(name)?;
        if !var.is_treated_as_nameref() {
            return Some(ResolvedVarRef {
                scope,
                variable: var,
                nameref_subscript: None,
            });
        }
        // Slow path: resolve nameref chain and re-lookup the target.
        let resolved = self.resolve_nameref_chain(name).ok()?;
        let (base, subscript) = parse_nameref_subscript(resolved.as_ref());
        let subscript_owned = subscript.map(|s| s.to_owned());
        let (scope, var) = self.get_by_exact_name(base)?;
        Some(ResolvedVarRef {
            scope,
            variable: var,
            nameref_subscript: subscript_owned,
        })
    }

    /// Looks up a variable by exact string name without nameref resolution.
    ///
    /// The name is used as a literal `HashMap` key — no subscript parsing, no
    /// nameref following. For subscripted targets like `"arr[2]"`, this does a
    /// literal lookup for the key `"arr[2]"` which won't match any variable,
    /// correctly terminating nameref chain resolution.
    fn get_by_exact_name<S: AsRef<str>>(&self, name: S) -> Option<(EnvironmentScope, &ShellVariable)> {
        // Look through scopes, from the top of the stack on down.
        for (scope_type, map) in self.scopes.iter().rev() {
            if let Some(var) = map.get(name.as_ref()) {
                return Some((*scope_type, var));
            }
        }

        None
    }

    /// Looks up a variable mutably, resolving namerefs transparently.
    ///
    /// Returns a [`ResolvedVarRefMut`] that provides safe access:
    /// - [`base_var_mut()`](ResolvedVarRefMut::base_var_mut) for attribute mutation
    /// - [`value_str()`](ResolvedVarRefMut::value_str) for reading the current value
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get_mut<S: AsRef<str>>(&mut self, name: S) -> Option<ResolvedVarRefMut<'_>> {
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
            let idx = found_scope_idx?;
            let (scope_type, map) = &mut self.scopes[idx];
            return map.get_mut(name).map(|var| ResolvedVarRefMut {
                scope: *scope_type,
                variable: var,
                nameref_subscript: None,
            });
        }
        // Slow path for namerefs.
        let resolved = self.resolve_nameref_chain(name).ok()?.into_owned();
        let (base, subscript) = parse_nameref_subscript(&resolved);
        let subscript_owned = subscript.map(|s| s.to_owned());
        let base = base.to_owned();
        let (scope, var) = self.get_mut_by_exact_name(base)?;
        Some(ResolvedVarRefMut {
            scope,
            variable: var,
            nameref_subscript: subscript_owned,
        })
    }

    /// Looks up a variable mutably by exact string name without nameref resolution.
    /// See [`get_by_exact_name`](Self::get_by_exact_name) for semantics.
    fn get_mut_by_exact_name<S: AsRef<str>>(
        &mut self,
        name: S,
    ) -> Option<(EnvironmentScope, &mut ShellVariable)> {
        // Look through scopes, from the top of the stack on down.
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if let Some(var) = map.get_mut(name.as_ref()) {
                return Some((*scope_type, var));
            }
        }

        None
    }

    /// Retrieves the string value of a variable, resolving namerefs and subscripts
    /// correctly.
    ///
    /// Convenience shorthand for `self.get(name)?.value_str(shell)`. Prefer
    /// [`ResolvedVarRef::value_str`] when you already have a resolved reference,
    /// or when you also need to inspect the variable's type/attributes.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    /// * `shell` - The shell owning the environment.
    pub fn get_str<S: AsRef<str>, SE: extensions::ShellExtensions>(
        &self,
        name: S,
        shell: &Shell<SE>,
    ) -> Option<Cow<'_, str>> {
        self.get(name)?.value_str(shell)
    }

    /// Checks if a variable of the given name is set in the environment,
    /// resolving namerefs transparently.
    ///
    /// For subscripted namerefs (e.g., `ref → arr[2]`), checks whether the
    /// specific element exists, not just the base array.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to check.
    /// * `shell` - The shell owning the environment (needed for subscripted
    ///   nameref element checks).
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
            if let Some(idx) = &resolved.nameref_subscript {
                value.has_element_at(idx, shell)
            } else {
                true
            }
        })
    }

    //
    // Setters
    //

    /// Tries to unset the variable with the given name in the environment, resolving
    /// namerefs transparently.
    ///
    /// Returns the removed [`ShellVariable`] when a whole variable is unset, or `None`
    /// if the variable was not found, the nameref was circular, or only an array element
    /// was removed (nameref-to-subscript targets like `arr[2]`).
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to unset.
    pub fn unset(&mut self, name: &str) -> Result<Option<ShellVariable>, error::Error> {
        // Resolve the nameref chain upfront, releasing the immutable borrow
        // on `self` before any mutation.
        let resolved = match self.resolve_nameref(name) {
            Ok(resolved) => resolved,
            Err(e) if matches!(e.kind(), error::ErrorKind::CircularNameReference(_)) => {
                // Circular nameref: bash removes the variable itself (by its
                // literal name) rather than following the chain.
                return self.unset_raw(name);
            }
            Err(e) => return Err(e),
        };

        if let Some(idx) = resolved.subscript() {
            // Name is already resolved — use get_mut_by_exact_name to avoid double resolution.
            if let Some((_, var)) = self.get_mut_by_exact_name(resolved.name()) {
                var.unset_index(idx)?;
            }
            return Ok(None);
        }

        self.unset_raw(resolved.name())
    }

    /// Unsets a variable by name, intentionally bypassing nameref resolution.
    ///
    /// Use this when the intent is to remove the variable itself rather than
    /// following nameref chains — e.g., `unset -n` removes the nameref variable,
    /// not its target.
    pub fn unset_bypassing_nameref(
        &mut self,
        name: &str,
    ) -> Result<Option<ShellVariable>, error::Error> {
        self.unset_raw(name)
    }

    /// Internal: unset by raw string name, no nameref resolution.
    fn unset_raw(&mut self, name: &str) -> Result<Option<ShellVariable>, error::Error> {
        let mut local_count = 0;
        for (scope_type, map) in self.scopes.iter_mut().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            let unset_result = Self::try_unset_in_map(map, name)?;

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
    /// nameref target. For example, `unset_index("ref", "3")` where `ref → arr[2]`
    /// unsets `arr[3]`, not `arr[2]`. If the name has already been resolved through
    /// the nameref chain, use [`get_mut_by_exact_name`](Self::get_mut_by_exact_name) + [`ShellVariable::unset_index`] directly
    /// to avoid double resolution.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the array variable to unset an element from.
    /// * `index` - The index of the element to unset.
    pub fn unset_index(&mut self, name: &str, index: &str) -> Result<bool, error::Error> {
        // The nameref subscript (e.g., ref→arr[2]) is intentionally ignored —
        // the explicit `index` argument takes precedence. See doc comment above.
        if let Some(mut resolved) = self.get_mut(name) {
            resolved.base_var_mut().unset_index(index)
        } else {
            Ok(false)
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

    /// Looks up a variable by exact string name with lookup policy, no nameref resolution.
    fn get_by_exact_name_using_policy<N: AsRef<str>>(
        &self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<(EnvironmentScope, &ShellVariable)> {
        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            match lookup_policy {
                EnvironmentLookup::Anywhere => (),
                EnvironmentLookup::OnlyInGlobal => {
                    if !matches!(scope_type, EnvironmentScope::Global) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInCurrentLocal => {
                    if !(matches!(scope_type, EnvironmentScope::Local) && local_count == 1) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInLocal => {
                    if !matches!(scope_type, EnvironmentScope::Local) {
                        continue;
                    }
                }
            }

            if let Some(var) = var_map.get(name.as_ref()) {
                return Some((*scope_type, var));
            }

            if matches!(scope_type, EnvironmentScope::Local)
                && matches!(lookup_policy, EnvironmentLookup::OnlyInCurrentLocal)
            {
                break;
            }
        }

        None
    }

    /// Looks up a variable mutably by exact string name with lookup policy, no nameref resolution.
    fn get_mut_by_exact_name_using_policy<N: AsRef<str>>(
        &mut self,
        name: N,
        lookup_policy: EnvironmentLookup,
    ) -> Option<(EnvironmentScope, &mut ShellVariable)> {
        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter_mut().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            match lookup_policy {
                EnvironmentLookup::Anywhere => (),
                EnvironmentLookup::OnlyInGlobal => {
                    if !matches!(scope_type, EnvironmentScope::Global) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInCurrentLocal => {
                    if !(matches!(scope_type, EnvironmentScope::Local) && local_count == 1) {
                        continue;
                    }
                }
                EnvironmentLookup::OnlyInLocal => {
                    if !matches!(scope_type, EnvironmentScope::Local) {
                        continue;
                    }
                }
            }

            if let Some(var) = var_map.get_mut(name.as_ref()) {
                return Some((*scope_type, var));
            }

            if matches!(scope_type, EnvironmentScope::Local)
                && matches!(lookup_policy, EnvironmentLookup::OnlyInCurrentLocal)
            {
                break;
            }
        }

        None
    }

    /// Update a variable in the environment, or add it if it doesn't already exist.
    /// Resolves namerefs transparently before performing the update.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to update or add.
    /// * `value` - The value to assign to the variable.
    /// * `updater` - A function to call to update the variable after assigning the value.
    /// * `lookup_policy` - The policy to use when looking up the variable.
    /// * `scope_if_creating` - The scope to create the variable in if it doesn't already exist.
    pub fn update_or_add<N: Into<String>>(
        &mut self,
        name: N,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let resolved = self.resolve_nameref(&name.into())?;

        // If the nameref target includes an array subscript (e.g., arr[2]),
        // redirect to the array-element update path for scalar values. Array
        // (compound) assignments through a subscripted nameref target the whole
        // base variable, matching bash behavior.
        if let Some(idx) = resolved.subscript() {
            let idx = idx.to_owned();
            let name = resolved.into_name();
            match value {
                variables::ShellValueLiteral::Scalar(scalar) => {
                    return self.update_or_add_array_element_raw(
                        name,
                        idx,
                        scalar,
                        updater,
                        lookup_policy,
                        scope_if_creating,
                    );
                }
                variables::ShellValueLiteral::Array(_) => {
                    return self.update_or_add_impl(
                        name,
                        value,
                        updater,
                        lookup_policy,
                        scope_if_creating,
                    );
                }
            }
        }

        self.update_or_add_impl(resolved.into_name(), value, updater, lookup_policy, scope_if_creating)
    }

    /// Update a variable in the environment, intentionally bypassing nameref
    /// resolution.
    ///
    /// Use this when the intent is to write to the variable itself rather than
    /// following the nameref chain — e.g., `for-in` loop control variables in
    /// bash update the nameref's own value, effectively retargeting it.
    pub fn update_or_add_bypassing_nameref<N: Into<String>>(
        &mut self,
        name: N,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        self.update_or_add_impl(name.into(), value, updater, lookup_policy, scope_if_creating)
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
    /// Resolves namerefs transparently before performing the update.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to update or add.
    /// * `index` - The index of the element to update or add.
    /// * `value` - The value to assign to the variable.
    /// * `updater` - A function to call to update the variable after assigning the value.
    /// * `lookup_policy` - The policy to use when looking up the variable.
    /// * `scope_if_creating` - The scope to create the variable in if it doesn't already exist.
    pub fn update_or_add_array_element<N: Into<String>>(
        &mut self,
        name: N,
        index: String,
        value: String,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let resolved = self.resolve_nameref(&name.into())?;

        // If the nameref target itself includes a subscript (e.g., ref→arr[2])
        // AND the caller provides an explicit index, the explicit index takes
        // precedence — matching `update_or_add`'s behavior. We use only the
        // base name and ignore the nameref's embedded subscript because the
        // caller's `index` argument is the authoritative subscript.
        self.update_or_add_array_element_raw(
            resolved.into_name(),
            index,
            value,
            updater,
            lookup_policy,
            scope_if_creating,
        )
    }

    /// Internal: update an array element where the name has already been resolved.
    fn update_or_add_array_element_raw(
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

    /// Adds a variable to the environment.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to add.
    /// * `var` - The variable to add.
    /// * `target_scope` - The scope to add the variable to.
    pub fn add<N: Into<String>>(
        &mut self,
        name: N,
        mut var: ShellVariable,
        target_scope: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let name = name.into();
        debug_assert!(
            !name.contains('['),
            "variable names must not contain '[': got '{name}'"
        );

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
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to set.
    /// * `var` - The variable to set.
    pub fn set_global<N: Into<String>>(
        &mut self,
        name: N,
        var: ShellVariable,
    ) -> Result<(), error::Error> {
        self.add(name, var, EnvironmentScope::Global)
    }
}

/// Represents a map from names to shell variables.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ShellVariableMap {
    variables: HashMap<String, ShellVariable>,
}

impl ShellVariableMap {
    //
    // Iterators/Getters
    //

    /// Returns an iterator over all the variables in the map.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ShellVariable)> {
        self.variables.iter()
    }

    /// Tries to retrieve an immutable reference to the variable with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get(&self, name: &str) -> Option<&ShellVariable> {
        self.variables.get(name)
    }

    /// Tries to retrieve a mutable reference to the variable with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to retrieve.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ShellVariable> {
        self.variables.get_mut(name)
    }

    //
    // Setters
    //

    /// Tries to unset the variable with the given name, returning the removed
    /// variable or None if it was not already set.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to unset.
    pub fn unset(&mut self, name: &str) -> Option<ShellVariable> {
        self.variables.remove(name)
    }

    /// Sets a variable in the map.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variable to set.
    /// * `var` - The variable to set.
    pub fn set<N: Into<String>>(&mut self, name: N, var: ShellVariable) -> Option<ShellVariable> {
        let name = name.into();
        debug_assert!(
            !name.contains('['),
            "variable names must not contain '[': got '{name}'"
        );
        self.variables.insert(name, var)
    }
}

/// Parse a potential `name[index]` subscript from a resolved nameref target string.
/// Returns `(base_name, Some(index))` if a subscript is present, or `(original, None)`.
///
/// Splits on the first `[` and requires a trailing `]`. Everything between the first
/// `[` and the final `]` is the index, which may contain arbitrary characters (including
/// nested brackets) for associative array keys.
///
/// This is a pure parser — callers are responsible for validating that the returned
/// base name is a valid variable name (see [`valid_variable_name`]).
/// Returns `true` if `target` is a valid nameref target name: the base name
/// (before any `[subscript]`) must be a legal variable name.
///
/// Does NOT check for self-references — callers must handle that separately.
pub fn valid_nameref_target_name(target: &str) -> bool {
    let (base, _) = parse_nameref_subscript(target);
    valid_variable_name(base)
}

pub(crate) fn parse_nameref_subscript(target: &str) -> (&str, Option<&str>) {
    // The target must end with `]` for a subscript to be present.
    let Some(without_bracket) = target.strip_suffix(']') else {
        return (target, None);
    };
    // Split on the first `[`. Everything before it is the variable name;
    // everything after (up to the stripped `]`) is the index.
    if let Some((name, index)) = without_bracket.split_once('[') {
        if !name.is_empty() {
            return (name, Some(index));
        }
    }
    (target, None)
}

/// Checks if the given name is a valid variable name.
pub fn valid_variable_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        Some(_) | None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract the string value of a variable for test assertions.
    /// Panics if the value is not a `ShellValue::String`.
    fn var_str(var: &ShellVariable) -> &str {
        match var.value() {
            ShellValue::String(s) => s.as_str(),
            other => panic!("expected ShellValue::String, got {other:?}"),
        }
    }

    #[test]
    fn test_valid_variable_name() {
        assert!(!valid_variable_name(""));
        assert!(!valid_variable_name("1"));
        assert!(!valid_variable_name(" a"));
        assert!(!valid_variable_name(" "));

        assert!(valid_variable_name("_"));
        assert!(valid_variable_name("_a"));
        assert!(valid_variable_name("_1"));
        assert!(valid_variable_name("_a1"));
        assert!(valid_variable_name("a"));
        assert!(valid_variable_name("A"));
        assert!(valid_variable_name("a1"));
        assert!(valid_variable_name("A1"));
    }

    //
    // parse_nameref_subscript
    //

    #[test]
    fn parse_nameref_subscript_no_subscript() {
        assert_eq!(parse_nameref_subscript("foo"), ("foo", None));
        assert_eq!(parse_nameref_subscript(""), ("", None));
    }

    #[test]
    fn parse_nameref_subscript_simple() {
        assert_eq!(parse_nameref_subscript("arr[2]"), ("arr", Some("2")));
        assert_eq!(parse_nameref_subscript("map[key]"), ("map", Some("key")));
    }

    #[test]
    fn parse_nameref_subscript_special_indices() {
        assert_eq!(parse_nameref_subscript("arr[@]"), ("arr", Some("@")));
        assert_eq!(parse_nameref_subscript("arr[*]"), ("arr", Some("*")));
        assert_eq!(parse_nameref_subscript("arr[-1]"), ("arr", Some("-1")));
    }

    #[test]
    fn parse_nameref_subscript_empty_brackets() {
        // Empty index — split_once returns ("arr", "") so we get Some("").
        // Higher-level code is responsible for rejecting empty subscripts.
        assert_eq!(parse_nameref_subscript("arr[]"), ("arr", Some("")));
    }

    #[test]
    fn parse_nameref_subscript_missing_open_bracket() {
        // No `[` to split on, but ends with `]` → not a subscript.
        assert_eq!(parse_nameref_subscript("foo]"), ("foo]", None));
    }

    #[test]
    fn parse_nameref_subscript_missing_close_bracket() {
        // Doesn't end with `]` → no subscript.
        assert_eq!(parse_nameref_subscript("arr[2"), ("arr[2", None));
    }

    #[test]
    fn parse_nameref_subscript_empty_name() {
        // `[idx]` with no name before bracket → not a subscript.
        assert_eq!(parse_nameref_subscript("[idx]"), ("[idx]", None));
    }

    #[test]
    fn parse_nameref_subscript_nested_brackets() {
        // Splits on the FIRST `[`; everything to the final `]` is the index.
        // This allows associative array keys to contain brackets.
        assert_eq!(parse_nameref_subscript("arr[a[b]]"), ("arr", Some("a[b]")));
        assert_eq!(parse_nameref_subscript("arr[[x]]"), ("arr", Some("[x]")));
    }

    //
    // ResolvedName construction & accessors
    //

    #[test]
    fn resolved_name_from_name_no_subscript() {
        let r = ResolvedName::already_resolved("target");
        assert_eq!(r.name(), "target");
        assert_eq!(r.subscript(), None);
    }

    #[test]
    fn resolved_name_parse_with_subscript() {
        let r = ResolvedName::parse("arr[5]".to_owned());
        assert_eq!(r.name(), "arr");
        assert_eq!(r.subscript(), Some("5"));
    }

    #[test]
    fn resolved_name_parse_without_subscript() {
        let r = ResolvedName::parse("plain".to_owned());
        assert_eq!(r.name(), "plain");
        assert_eq!(r.subscript(), None);
    }

    #[test]
    fn resolved_name_without_subscript_strips_index() {
        let r = ResolvedName::parse("arr[2]".to_owned());
        let stripped = r.without_subscript();
        assert_eq!(stripped.name(), "arr");
        assert_eq!(stripped.subscript(), None);
        // Original is unchanged.
        assert_eq!(r.subscript(), Some("2"));
    }

    #[test]
    fn resolved_name_into_name_consumes() {
        let r = ResolvedName::parse("arr[k]".to_owned());
        assert_eq!(r.into_name(), "arr");
    }

    //
    // resolve_nameref_chain — using a real ShellEnvironment
    //

    /// Create a non-nameref string variable.
    fn make_var(value: &str) -> ShellVariable {
        ShellVariable::new(ShellValue::String(value.to_owned()))
    }

    /// Create a nameref variable pointing to `target`.
    fn make_nameref(target: &str) -> ShellVariable {
        let mut v = ShellVariable::new(ShellValue::String(target.to_owned()));
        v.treat_as_nameref();
        v
    }

    #[test]
    fn resolve_nameref_identity_on_non_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("plain", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref("plain").unwrap();
        assert_eq!(r.name(), "plain");
        assert_eq!(r.subscript(), None);
    }

    #[test]
    fn resolve_nameref_identity_on_missing_var() {
        let env = ShellEnvironment::new();
        let r = env.resolve_nameref("nonexistent").unwrap();
        assert_eq!(r.name(), "nonexistent");
        assert_eq!(r.subscript(), None);
    }

    #[test]
    fn resolve_nameref_single_hop() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref("ref").unwrap();
        assert_eq!(r.name(), "target");
        assert_eq!(r.subscript(), None);
    }

    #[test]
    fn resolve_nameref_chain_three_hops() {
        let mut env = ShellEnvironment::new();
        env.add("ultimate", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        env.add("middle", make_nameref("ultimate"), EnvironmentScope::Global)
            .unwrap();
        env.add("top", make_nameref("middle"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref("top").unwrap();
        assert_eq!(r.name(), "ultimate");
    }

    #[test]
    fn resolve_nameref_chain_to_subscripted_target() {
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref("arr[2]"), EnvironmentScope::Global)
            .unwrap();
        // Note: we don't need `arr` to exist for resolution; the chain
        // terminates because `arr[2]` isn't a registered name.
        let r = env.resolve_nameref("ref").unwrap();
        assert_eq!(r.name(), "arr");
        assert_eq!(r.subscript(), Some("2"));
    }

    #[test]
    fn resolve_nameref_self_reference_is_circular() {
        let mut env = ShellEnvironment::new();
        env.add("self", make_nameref("self"), EnvironmentScope::Global)
            .unwrap();
        let err = env.resolve_nameref("self").unwrap_err();
        assert!(matches!(
            err.kind(),
            error::ErrorKind::CircularNameReference(_)
        ));
    }

    #[test]
    fn resolve_nameref_two_node_cycle_is_circular() {
        let mut env = ShellEnvironment::new();
        env.add("a", make_nameref("b"), EnvironmentScope::Global)
            .unwrap();
        env.add("b", make_nameref("a"), EnvironmentScope::Global)
            .unwrap();
        let err = env.resolve_nameref("a").unwrap_err();
        assert!(matches!(
            err.kind(),
            error::ErrorKind::CircularNameReference(_)
        ));
    }

    #[test]
    fn resolve_nameref_three_node_cycle_is_circular() {
        let mut env = ShellEnvironment::new();
        env.add("c1", make_nameref("c2"), EnvironmentScope::Global)
            .unwrap();
        env.add("c2", make_nameref("c3"), EnvironmentScope::Global)
            .unwrap();
        env.add("c3", make_nameref("c1"), EnvironmentScope::Global)
            .unwrap();
        let err = env.resolve_nameref("c1").unwrap_err();
        assert!(matches!(
            err.kind(),
            error::ErrorKind::CircularNameReference(_)
        ));
    }

    #[test]
    fn resolve_nameref_at_max_depth_succeeds() {
        // Build a chain of exactly MAX_NAMEREF_DEPTH hops terminating in a
        // non-nameref. Should succeed.
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        // Names: link0 -> link1 -> ... -> link{N-2} -> target.
        // That's MAX_NAMEREF_DEPTH - 1 nameref links plus the terminal lookup.
        // We need a chain that exercises the depth limit without exceeding it.
        let mut prev = "target".to_owned();
        for i in 0..MAX_NAMEREF_DEPTH - 1 {
            let name = format!("link{i}");
            env.add(&name, make_nameref(&prev), EnvironmentScope::Global)
                .unwrap();
            prev = name;
        }
        // Resolve from the deepest link.
        let r = env.resolve_nameref(&prev).unwrap();
        assert_eq!(r.name(), "target");
    }

    #[test]
    fn resolve_nameref_beyond_max_depth_errors() {
        // Build a chain of MAX_NAMEREF_DEPTH + 2 namerefs, all pointing onward.
        // The chain is acyclic but too long — should hit the depth limit.
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        let mut prev = "target".to_owned();
        for i in 0..MAX_NAMEREF_DEPTH + 2 {
            let name = format!("link{i}");
            env.add(&name, make_nameref(&prev), EnvironmentScope::Global)
                .unwrap();
            prev = name;
        }
        let err = env.resolve_nameref(&prev).unwrap_err();
        assert!(matches!(
            err.kind(),
            error::ErrorKind::CircularNameReference(_)
        ));
    }

    #[test]
    fn resolve_nameref_to_name_strips_no_subscript() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();
        // Plain target — name should be "target", no subscript handling.
        assert_eq!(env.resolve_nameref_to_name("ref").unwrap(), "target");
    }

    #[test]
    fn resolve_nameref_to_name_preserves_subscript() {
        // resolve_nameref_to_name returns the resolved string AS-IS, without
        // parsing any subscript out of it. This is important for `[[ -v ref ]]`
        // semantics where bash treats "arr[2]" literally as a variable name.
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref("arr[2]"), EnvironmentScope::Global)
            .unwrap();
        assert_eq!(env.resolve_nameref_to_name("ref").unwrap(), "arr[2]");
    }

    #[test]
    fn resolve_nameref_with_empty_target_terminates() {
        // A nameref pointing at an empty string isn't followed — bash treats
        // it as the identity (the nameref's own name).
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref(""), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref("ref").unwrap();
        assert_eq!(r.name(), "ref");
        assert_eq!(r.subscript(), None);
    }

    //
    // resolve_nameref_or_default
    //

    #[test]
    fn resolve_nameref_or_default_returns_resolved_on_success() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref_or_default("ref");
        assert_eq!(r.name(), "target");
    }

    #[test]
    fn resolve_nameref_or_default_returns_identity_on_circular() {
        let mut env = ShellEnvironment::new();
        env.add("a", make_nameref("b"), EnvironmentScope::Global)
            .unwrap();
        env.add("b", make_nameref("a"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref_or_default("a");
        assert_eq!(r.name(), "a");
        assert_eq!(r.subscript(), None);
    }

    //
    // valid_nameref_target_name
    //

    #[test]
    fn valid_nameref_target_simple() {
        assert!(valid_nameref_target_name("foo"));
        assert!(valid_nameref_target_name("_bar"));
        assert!(valid_nameref_target_name("arr[2]"));
        assert!(valid_nameref_target_name("arr[@]"));
    }

    #[test]
    fn invalid_nameref_target() {
        assert!(!valid_nameref_target_name(""));
        assert!(!valid_nameref_target_name("1bad"));
        assert!(!valid_nameref_target_name("[idx]"));
    }

    //
    // Lookup builder API
    //

    #[test]
    fn lookup_str_auto_resolves_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        // lookup("ref").get() should resolve through the nameref to "target".
        let resolved = env.lookup("ref").get().expect("should find target");
        assert_eq!(resolved.scope(), EnvironmentScope::Global);
        // base_var() should be the target variable, not the nameref.
        assert_eq!(var_str(resolved.base_var()), "hello");
        assert!(!resolved.has_subscript());
    }

    #[test]
    fn lookup_str_bypassing_nameref_returns_nameref_itself() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        // lookup("ref").bypassing_nameref().get() should return the nameref variable.
        let (scope, var) = env
            .lookup("ref")
            .bypassing_nameref()
            .get()
            .expect("should find ref");
        assert_eq!(scope, EnvironmentScope::Global);
        assert!(var.is_treated_as_nameref());
        assert_eq!(var_str(var), "target");
    }

    #[test]
    fn lookup_resolved_name_skips_resolution() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();

        let resolved = ResolvedName::already_resolved("target");
        let (scope, var) = env
            .lookup(&resolved)
            .get()
            .expect("should find target");
        assert_eq!(scope, EnvironmentScope::Global);
        assert_eq!(var_str(var), "hello");
    }

    #[test]
    fn lookup_in_scope_restricts_to_local() {
        let mut env = ShellEnvironment::new();
        env.add("x", make_var("global"), EnvironmentScope::Global)
            .unwrap();
        env.push_scope(EnvironmentScope::Local);
        // "x" exists in global but NOT in current local.
        assert!(env
            .lookup("x")
            .bypassing_nameref()
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get()
            .is_none());
        // But it IS visible with Anywhere.
        assert!(env
            .lookup("x")
            .bypassing_nameref()
            .in_scope(EnvironmentLookup::Anywhere)
            .get()
            .is_some());
        env.pop_scope(EnvironmentScope::Local).unwrap();
    }

    #[test]
    fn lookup_in_scope_finds_local() {
        let mut env = ShellEnvironment::new();
        env.add("x", make_var("global"), EnvironmentScope::Global)
            .unwrap();
        env.push_scope(EnvironmentScope::Local);
        env.add("x", make_var("local"), EnvironmentScope::Local)
            .unwrap();

        let (scope, var) = env
            .lookup("x")
            .bypassing_nameref()
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get()
            .expect("should find local x");
        assert_eq!(scope, EnvironmentScope::Local);
        assert_eq!(var_str(var), "local");
        env.pop_scope(EnvironmentScope::Local).unwrap();
    }

    #[test]
    fn lookup_mut_auto_resolves_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("original"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        // Mutating through nameref should affect the target.
        let resolved = env
            .lookup_mut("ref")
            .get()
            .expect("should find target");
        assert_eq!(resolved.scope(), EnvironmentScope::Global);
        assert!(!resolved.has_subscript());
    }

    #[test]
    fn lookup_mut_bypassing_nameref_returns_nameref_itself() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        let (scope, var) = env
            .lookup_mut("ref")
            .bypassing_nameref()
            .get()
            .expect("should find ref");
        assert_eq!(scope, EnvironmentScope::Global);
        assert!(var.is_treated_as_nameref());
    }

    #[test]
    fn lookup_nonexistent_returns_none() {
        let env = ShellEnvironment::new();
        assert!(env.lookup("nonexistent").get().is_none());
        assert!(env
            .lookup("nonexistent")
            .bypassing_nameref()
            .get()
            .is_none());
        let resolved = ResolvedName::already_resolved("nonexistent");
        assert!(env.lookup(&resolved).get().is_none());
    }

    #[test]
    fn lookup_str_auto_resolve_with_subscripted_nameref() {
        let mut env = ShellEnvironment::new();
        let arr = ShellVariable::new(ShellValue::indexed_array_from_strs(&[
            "zero", "one", "two",
        ]));
        env.add("arr", arr, EnvironmentScope::Global).unwrap();
        env.add("ref", make_nameref("arr[1]"), EnvironmentScope::Global)
            .unwrap();

        let resolved = env.lookup("ref").get().expect("should find arr");
        assert!(resolved.has_subscript());
        // base_var() should be the array itself.
        assert!(matches!(
            resolved.base_var().value(),
            ShellValue::IndexedArray(_)
        ));
    }

    #[test]
    fn lookup_circular_nameref_returns_none() {
        let mut env = ShellEnvironment::new();
        env.add("a", make_nameref("b"), EnvironmentScope::Global)
            .unwrap();
        env.add("b", make_nameref("a"), EnvironmentScope::Global)
            .unwrap();
        // Auto-resolving lookup silently returns None for circular namerefs.
        assert!(env.lookup("a").get().is_none());
    }

    #[test]
    fn lookup_resolved_name_with_in_scope() {
        let mut env = ShellEnvironment::new();
        env.add("x", make_var("global"), EnvironmentScope::Global)
            .unwrap();
        env.push_scope(EnvironmentScope::Local);
        env.add("x", make_var("local"), EnvironmentScope::Local)
            .unwrap();

        let resolved = ResolvedName::already_resolved("x");

        // OnlyInGlobal should find the global one.
        let (scope, var) = env
            .lookup(&resolved)
            .in_scope(EnvironmentLookup::OnlyInGlobal)
            .get()
            .expect("should find global x");
        assert_eq!(scope, EnvironmentScope::Global);
        assert_eq!(var_str(var), "global");

        // OnlyInCurrentLocal should find the local one.
        let (scope, var) = env
            .lookup(&resolved)
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get()
            .expect("should find local x");
        assert_eq!(scope, EnvironmentScope::Local);
        assert_eq!(var_str(var), "local");

        env.pop_scope(EnvironmentScope::Local).unwrap();
    }
}
