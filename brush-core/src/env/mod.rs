//! Implements a shell variable environment.

mod names;

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map;

use names::parse_nameref_subscript;

use crate::Shell;
use crate::error;
use crate::extensions;
use crate::variables::{self, ShellValue, ShellValueUnsetType, ShellVariable};

pub use names::{
    ResolvedName, VarName, VarNameExt, valid_nameref_target_name, valid_variable_name,
};

/// Maximum depth for nameref chain resolution. Matches bash 5.2's internal
/// `NAMEREF_MAX` limit (8). Prevents infinite loops on pathological chains
/// and guards against stack-like resource exhaustion.
const MAX_NAMEREF_DEPTH: usize = 8;

// ─── Variable lookup API design ─────────────────────────────────────
//
// The central input type is `VarName`, which encodes resolution strategy:
//
//   VarName::Auto("name")      → follow nameref chains
//   VarName::Resolved { .. }   → already resolved, look up directly
//   VarName::Direct("name")    → bypass nameref resolution
//
// `&str` and `String` convert to `VarName::Auto` by default.
// `ResolvedName` converts to `VarName::Resolved`.
// Use `VarName::direct("name")` for the bypass case.
//
// Mutation methods (update_or_add, unset, etc.) take `impl Into<VarName>`,
// eliminating the old `_bypassing_nameref` method pairs.
//
// For scope-restricted lookups, use the `lookup()` / `lookup_mut()` builders.
// ────────────────────────────────────────────────────────────────────

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
// Entry points: `lookup()` and `lookup_mut()`, accepting `impl Into<VarName>`.
//
// Usage:
//   env.lookup("name").get()                          // auto-resolve → ResolvedVarRef
//   env.lookup(VarName::direct("name")).get_direct()   // bypass → (Scope, &Var)
//   env.lookup(resolved).get()                         // pre-resolved → ResolvedVarRef
//   env.lookup(resolved).in_scope(policy).get()        // pre-resolved + scoped
//   env.lookup("name").in_scope(policy).get_direct()   // scoped direct lookup
// ────────────────────────────────────────────────────────────────────

/// Immutable lookup builder driven by [`VarName`].
pub struct VarLookup<'a> {
    env: &'a ShellEnvironment,
    name: VarName,
    policy: EnvironmentLookup,
}

impl<'a> VarLookup<'a> {
    /// Restrict the lookup to a specific scope.
    #[must_use]
    pub const fn in_scope(mut self, policy: EnvironmentLookup) -> Self {
        self.policy = policy;
        self
    }

    /// Execute the lookup, resolving namerefs as specified by the [`VarName`] variant.
    ///
    /// - `VarName::Auto` — follows nameref chains transparently.
    /// - `VarName::Resolved` — looks up the pre-resolved base name directly.
    /// - `VarName::Direct` — looks up the variable directly, no resolution.
    pub fn get(self) -> Option<ResolvedVarRef<'a>> {
        match &self.name {
            VarName::Auto(s) => self.env.get_auto(s),
            VarName::Resolved { base, subscript } => {
                let (scope, var) = self.env.get_by_exact_name_using_policy(base, self.policy)?;
                Some(ResolvedVarRef {
                    scope,
                    variable: var,
                    nameref_subscript: subscript.clone(),
                })
            }
            VarName::Direct(s) => {
                let (scope, var) = self.env.get_by_exact_name_using_policy(s, self.policy)?;
                Some(ResolvedVarRef {
                    scope,
                    variable: var,
                    nameref_subscript: None,
                })
            }
        }
    }

    /// Look up the variable directly without subscript handling.
    ///
    /// Returns the raw `(Scope, &ShellVariable)` pair. This is the replacement
    /// for the old `.bypassing_nameref().get()` pattern — use it to inspect the
    /// variable itself (e.g., checking nameref attribute, `declare -p`).
    pub fn get_direct(self) -> Option<(EnvironmentScope, &'a ShellVariable)> {
        let key = self.name.as_lookup_key();
        self.env.get_by_exact_name_using_policy(key, self.policy)
    }
}

/// Mutable lookup builder driven by [`VarName`].
pub struct VarLookupMut<'a> {
    env: &'a mut ShellEnvironment,
    name: VarName,
    policy: EnvironmentLookup,
}

impl<'a> VarLookupMut<'a> {
    /// Restrict the lookup to a specific scope.
    #[must_use]
    pub const fn in_scope(mut self, policy: EnvironmentLookup) -> Self {
        self.policy = policy;
        self
    }

    /// Execute the mutable lookup, resolving namerefs as specified by the [`VarName`] variant.
    pub fn get(self) -> Option<ResolvedVarRefMut<'a>> {
        match &self.name {
            VarName::Auto(s) => {
                let s = s.clone();
                self.env.get_mut_auto(&s)
            }
            VarName::Resolved { base, subscript } => {
                let (scope, var) = self
                    .env
                    .get_mut_by_exact_name_using_policy(base, self.policy)?;
                Some(ResolvedVarRefMut {
                    scope,
                    variable: var,
                    nameref_subscript: subscript.clone(),
                })
            }
            VarName::Direct(s) => {
                let (scope, var) = self
                    .env
                    .get_mut_by_exact_name_using_policy(s, self.policy)?;
                Some(ResolvedVarRefMut {
                    scope,
                    variable: var,
                    nameref_subscript: None,
                })
            }
        }
    }

    /// Mutable direct lookup without subscript handling.
    ///
    /// See [`VarLookup::get_direct`] for the immutable counterpart.
    pub fn get_direct(self) -> Option<(EnvironmentScope, &'a mut ShellVariable)> {
        let key = self.name.as_lookup_key().to_owned();
        self.env
            .get_mut_by_exact_name_using_policy(&key, self.policy)
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
    fn resolve_nameref_chain<'a>(&'a self, name: &'a str) -> Result<Cow<'a, str>, error::Error> {
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
                return Err(error::ErrorKind::CircularNameReference(current.to_owned()).into());
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
                            return Err(error::ErrorKind::CircularNameReference(
                                current.to_owned(),
                            )
                            .into());
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

    /// Creates an immutable lookup builder.
    ///
    /// Accepts anything that converts to [`VarName`] — `&str` (auto-resolve),
    /// [`ResolvedName`] (pre-resolved), or `VarName::direct(name)` (bypass).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// env.lookup("name").get()                          // → Option<ResolvedVarRef>
    /// env.lookup(VarName::direct("name")).get_direct()   // → Option<(Scope, &Var)>
    /// env.lookup(resolved).get()                         // → Option<ResolvedVarRef>
    /// env.lookup(resolved).in_scope(policy).get()        // → Option<ResolvedVarRef>
    /// ```
    pub fn lookup<N: Into<VarName>>(&self, name: N) -> VarLookup<'_> {
        VarLookup {
            env: self,
            name: name.into(),
            policy: EnvironmentLookup::Anywhere,
        }
    }

    /// Creates a mutable lookup builder.
    ///
    /// Accepts anything that converts to [`VarName`] — `&str` (auto-resolve),
    /// [`ResolvedName`] (pre-resolved), or `VarName::direct(name)` (bypass).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// env.lookup_mut("name").get()                          // → Option<ResolvedVarRefMut>
    /// env.lookup_mut(VarName::direct("name")).get_direct()   // → Option<(Scope, &mut Var)>
    /// env.lookup_mut(resolved).get()                         // → Option<ResolvedVarRefMut>
    /// ```
    pub fn lookup_mut<N: Into<VarName>>(&mut self, name: N) -> VarLookupMut<'_> {
        VarLookupMut {
            env: self,
            name: name.into(),
            policy: EnvironmentLookup::Anywhere,
        }
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
        self.get_auto(name.as_ref())
    }

    /// Auto-resolving lookup used by `get()` and `VarLookup::get()`.
    fn get_auto(&self, name: &str) -> Option<ResolvedVarRef<'_>> {
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
    fn get_by_exact_name<S: AsRef<str>>(
        &self,
        name: S,
    ) -> Option<(EnvironmentScope, &ShellVariable)> {
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
        self.get_mut_auto(name.as_ref())
    }

    /// Auto-resolving mutable lookup used by `get_mut()` and `VarLookupMut::get()`.
    fn get_mut_auto(&mut self, name: &str) -> Option<ResolvedVarRefMut<'_>> {
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

    /// Tries to unset the variable with the given name in the environment.
    ///
    /// Behavior depends on the [`VarName`] variant:
    /// - `VarName::Auto` — resolves namerefs, unsets the target. On circular
    ///   namerefs, falls back to unsetting the variable itself.
    /// - `VarName::Resolved` — unsets by the pre-resolved base name.
    /// - `VarName::Direct` — unsets the variable itself, bypassing namerefs.
    ///
    /// Returns the removed [`ShellVariable`] when a whole variable is unset, or `None`
    /// if the variable was not found or only an array element was removed.
    pub fn unset(
        &mut self,
        name: impl Into<VarName>,
    ) -> Result<Option<ShellVariable>, error::Error> {
        match name.into() {
            VarName::Auto(s) => {
                let resolved = match self.resolve_nameref(&s) {
                    Ok(r) => r,
                    Err(e) if matches!(e.kind(), error::ErrorKind::CircularNameReference(_)) => {
                        return self.unset_direct(&s);
                    }
                    Err(e) => return Err(e),
                };
                if let Some(idx) = resolved.subscript() {
                    if let Some((_, var)) = self.get_mut_by_exact_name(resolved.name()) {
                        var.unset_index(idx)?;
                    }
                    return Ok(None);
                }
                self.unset_direct(resolved.name())
            }
            VarName::Resolved { base, subscript } => {
                if let Some(idx) = subscript {
                    if let Some((_, var)) = self.get_mut_by_exact_name(&base) {
                        var.unset_index(&idx)?;
                    }
                    return Ok(None);
                }
                self.unset_direct(&base)
            }
            VarName::Direct(s) => self.unset_direct(&s),
        }
    }

    /// Unsets a variable by exact name, no nameref resolution.
    fn unset_direct(&mut self, name: &str) -> Result<Option<ShellVariable>, error::Error> {
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
    ///
    /// Behavior depends on the [`VarName`] variant:
    /// - `VarName::Auto` — resolves namerefs, writes to the target.
    /// - `VarName::Resolved` — writes to the pre-resolved base name.
    /// - `VarName::Direct` — writes to the variable itself, bypassing namerefs.
    pub fn update_or_add(
        &mut self,
        name: impl Into<VarName>,
        value: variables::ShellValueLiteral,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let var_name = name.into();
        let (base, subscript) = match &var_name {
            VarName::Auto(s) => {
                let resolved = self.resolve_nameref(s)?;
                (resolved.name.clone(), resolved.subscript)
            }
            VarName::Resolved { base, subscript } => (base.clone(), subscript.clone()),
            VarName::Direct(s) => (s.clone(), None),
        };

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
                    return self.update_or_add_impl(
                        base,
                        value,
                        updater,
                        lookup_policy,
                        scope_if_creating,
                    );
                }
            }
        }

        self.update_or_add_impl(base, value, updater, lookup_policy, scope_if_creating)
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
    /// Behavior depends on the [`VarName`] variant:
    /// - `VarName::Auto` — resolves namerefs, writes to the target.
    /// - `VarName::Resolved` — writes to the pre-resolved base name.
    /// - `VarName::Direct` — writes to the variable itself.
    ///
    /// The explicit `index` parameter always takes precedence over any subscript
    /// embedded in a nameref target.
    pub fn update_or_add_array_element(
        &mut self,
        name: impl Into<VarName>,
        index: String,
        value: String,
        updater: impl Fn(&mut ShellVariable) -> Result<(), error::Error>,
        lookup_policy: EnvironmentLookup,
        scope_if_creating: EnvironmentScope,
    ) -> Result<(), error::Error> {
        let var_name = name.into();
        let base = match &var_name {
            VarName::Auto(s) => self.resolve_nameref(s)?.into_name(),
            VarName::Resolved { base, .. } => base.clone(),
            VarName::Direct(s) => s.clone(),
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

    /// Shared implementation for array element updates.
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn var_str(var: &ShellVariable) -> &str {
        match var.value() {
            ShellValue::String(s) => s.as_str(),
            other => panic!("expected ShellValue::String, got {other:?}"),
        }
    }

    fn make_var(value: &str) -> ShellVariable {
        ShellVariable::new(ShellValue::String(value.to_owned()))
    }

    fn make_nameref(target: &str) -> ShellVariable {
        let mut v = ShellVariable::new(ShellValue::String(target.to_owned()));
        v.treat_as_nameref();
        v
    }

    //
    // resolve_nameref_chain
    //

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
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        let mut prev = "target".to_owned();
        for i in 0..MAX_NAMEREF_DEPTH - 1 {
            let name = format!("link{i}");
            env.add(&name, make_nameref(&prev), EnvironmentScope::Global)
                .unwrap();
            prev = name;
        }
        let r = env.resolve_nameref(&prev).unwrap();
        assert_eq!(r.name(), "target");
    }

    #[test]
    fn resolve_nameref_beyond_max_depth_errors() {
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
        assert_eq!(env.resolve_nameref_to_name("ref").unwrap(), "target");
    }

    #[test]
    fn resolve_nameref_to_name_preserves_subscript() {
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref("arr[2]"), EnvironmentScope::Global)
            .unwrap();
        assert_eq!(env.resolve_nameref_to_name("ref").unwrap(), "arr[2]");
    }

    #[test]
    fn resolve_nameref_with_empty_target_terminates() {
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref(""), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref("ref").unwrap();
        assert_eq!(r.name(), "ref");
        assert_eq!(r.subscript(), None);
    }

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
    // Lookup builder API
    //

    #[test]
    fn lookup_str_auto_resolves_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        let resolved = env.lookup("ref").get().expect("should find target");
        assert_eq!(resolved.scope(), EnvironmentScope::Global);
        assert_eq!(var_str(resolved.base_var()), "hello");
        assert!(!resolved.has_subscript());
    }

    #[test]
    fn lookup_direct_returns_nameref_itself() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        let (scope, var) = env
            .lookup(VarName::direct("ref"))
            .get_direct()
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
        let result = env.lookup(&resolved).get().expect("should find target");
        assert_eq!(result.scope(), EnvironmentScope::Global);
        assert_eq!(var_str(result.base_var()), "hello");
    }

    #[test]
    fn lookup_in_scope_restricts_to_local() {
        let mut env = ShellEnvironment::new();
        env.add("x", make_var("global"), EnvironmentScope::Global)
            .unwrap();
        env.push_scope(EnvironmentScope::Local);
        assert!(
            env.lookup(VarName::direct("x"))
                .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
                .get_direct()
                .is_none()
        );
        assert!(
            env.lookup(VarName::direct("x"))
                .in_scope(EnvironmentLookup::Anywhere)
                .get_direct()
                .is_some()
        );
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
            .lookup(VarName::direct("x"))
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get_direct()
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

        let resolved = env.lookup_mut("ref").get().expect("should find target");
        assert_eq!(resolved.scope(), EnvironmentScope::Global);
        assert!(!resolved.has_subscript());
    }

    #[test]
    fn lookup_mut_direct_returns_nameref_itself() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        let (scope, var) = env
            .lookup_mut(VarName::direct("ref"))
            .get_direct()
            .expect("should find ref");
        assert_eq!(scope, EnvironmentScope::Global);
        assert!(var.is_treated_as_nameref());
    }

    #[test]
    fn lookup_nonexistent_returns_none() {
        let env = ShellEnvironment::new();
        assert!(env.lookup("nonexistent").get().is_none());
        assert!(
            env.lookup(VarName::direct("nonexistent"))
                .get_direct()
                .is_none()
        );
        let resolved = ResolvedName::already_resolved("nonexistent");
        assert!(env.lookup(&resolved).get().is_none());
    }

    #[test]
    fn lookup_str_auto_resolve_with_subscripted_nameref() {
        let mut env = ShellEnvironment::new();
        let arr = ShellVariable::new(ShellValue::indexed_array_from_strs(&["zero", "one", "two"]));
        env.add("arr", arr, EnvironmentScope::Global).unwrap();
        env.add("ref", make_nameref("arr[1]"), EnvironmentScope::Global)
            .unwrap();

        let resolved = env.lookup("ref").get().expect("should find arr");
        assert!(resolved.has_subscript());
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

        let (scope, var) = env
            .lookup(&resolved)
            .in_scope(EnvironmentLookup::OnlyInGlobal)
            .get_direct()
            .expect("should find global x");
        assert_eq!(scope, EnvironmentScope::Global);
        assert_eq!(var_str(var), "global");

        let (scope, var) = env
            .lookup(&resolved)
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get_direct()
            .expect("should find local x");
        assert_eq!(scope, EnvironmentScope::Local);
        assert_eq!(var_str(var), "local");

        env.pop_scope(EnvironmentScope::Local).unwrap();
    }

    //
    // VarName-based unset
    //

    #[test]
    fn unset_auto_resolves_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        env.unset("ref").unwrap();
        assert!(env.get("target").is_none());
        // ref still exists as a nameref, but its target is gone.
        let (scope, var) = env
            .lookup(VarName::direct("ref"))
            .get_direct()
            .expect("ref still exists");
        assert_eq!(scope, EnvironmentScope::Global);
        assert!(var.is_treated_as_nameref());
    }

    #[test]
    fn unset_direct_bypasses_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        env.unset(VarName::direct("ref")).unwrap();
        assert!(env.get("target").is_some());
        assert!(env.lookup(VarName::direct("ref")).get_direct().is_none());
    }

    //
    // VarName-based update_or_add
    //

    #[test]
    fn update_or_add_auto_resolves_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        env.update_or_add(
            "ref",
            variables::ShellValueLiteral::Scalar("hello".to_owned()),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )
        .unwrap();

        assert_eq!(
            var_str(env.get("target").expect("target should exist").base_var()),
            "hello"
        );
    }

    #[test]
    fn update_or_add_direct_bypasses_nameref() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("original"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        env.update_or_add(
            VarName::direct("ref"),
            variables::ShellValueLiteral::Scalar("retargeted".to_owned()),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )
        .unwrap();

        assert_eq!(
            var_str(env.get("target").expect("target unchanged").base_var()),
            "original"
        );
        // ref's own value was updated (bypassing nameref resolution).
        let (_, var) = env
            .lookup(VarName::direct("ref"))
            .get_direct()
            .expect("ref exists");
        assert_eq!(var_str(var), "retargeted");
    }
}
