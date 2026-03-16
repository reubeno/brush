//! Implements a shell variable environment.
//!
//! Reads go through [`ShellEnvironment::lookup`] (auto-resolving) or
//! [`ShellEnvironment::lookup_resolved`] (pre-resolved). Mutations go through
//! [`ShellEnvironment::update_or_add`] / [`ShellEnvironment::unset`] — or
//! [`ShellEnvironment::set_var`] for the common case. See [`NameRef`] for the
//! three mutation dispatch strategies.

mod lookup;
mod mutation;
mod names;
mod scope;
mod var_map;

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map;

use crate::error;
use crate::variables::{ShellValue, ShellVariable};

pub use lookup::{
    DirectVarLookup, DirectVarLookupMut, ResolvedVarRef, ResolvedVarRefMut, VarLookup, VarLookupMut,
};
pub use names::{NameRef, ResolvedName, valid_nameref_target_name, valid_variable_name};
pub(crate) use scope::ScopeGuard;
pub use scope::{EnvironmentLookup, EnvironmentScope};
pub use var_map::ShellVariableMap;

/// Maximum depth for nameref chain resolution. Matches bash 5.2's internal
/// `NAMEREF_MAX` limit (8).
const MAX_NAMEREF_DEPTH: usize = 8;

/// Represents the shell variable environment, composed of a stack of scopes.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ShellEnvironment {
    /// Stack of scopes, with the top of the stack being the current scope.
    pub(super) scopes: Vec<(EnvironmentScope, ShellVariableMap)>,
    /// Whether or not to auto-export variables on creation or modification.
    pub(super) export_variables_on_modification: bool,
    /// Count of total entries (may include duplicates with shadowed variables).
    pub(super) entry_count: usize,
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
        // A scope-type mismatch indicates a broken push/pop invariant. We
        // return Err (rather than panic) so embedders can recover; in practice
        // this should never trigger.
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
    // Three public methods, all walking the same chain — they differ only in
    // what they do with the final string:
    //   resolve_nameref          → ResolvedName, parses "arr[2]" into base + subscript
    //   resolve_nameref_unparsed → String, returns the final string as-is (used
    //                              for `[[ -v ref ]]` where bash takes the
    //                              resolved target as a literal variable name)
    //   resolve_nameref_chain    → Cow<str>, lowest-level (private)
    //
    // For *lookups* that resolve namerefs and want subscript-aware access,
    // prefer the lookup builders in `lookup.rs` (`get` / `get_mut`).
    //

    /// Resolves a nameref chain, returning the final target string. Errors on
    /// circular references and depth overflow.
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

    /// Resolves a nameref chain and parses any subscript from the final target.
    /// `ref→"arr[2]"` returns `ResolvedName { name: "arr", subscript: Some("2") }`.
    pub fn resolve_nameref(&self, name: &str) -> Result<ResolvedName, error::Error> {
        let resolved = self.resolve_nameref_chain(name)?;
        Ok(ResolvedName::parse(resolved.into_owned()))
    }

    /// Resolves a nameref chain, returning the final target string verbatim
    /// (no subscript parsing). For `[[ -v ref ]]` semantics, where bash treats
    /// `arr[2]` as a literal variable name.
    pub fn resolve_nameref_unparsed(&self, name: &str) -> Result<String, error::Error> {
        let resolved = self.resolve_nameref_chain(name)?;
        Ok(resolved.into_owned())
    }

    //
    // Circular-nameref handling
    //
    // When `resolve_nameref` hits a cycle, callers pick one of four policies:
    //   1. Warn + identity (value expansion) — emit "warning: ref: circular
    //      name reference", then treat as identity for the value lookup.
    //      `WordExpander::resolve_nameref_or_self` in expansion.rs.
    //   2. Warn + skip (writes through nameref) — `apply_assignment`,
    //      `assign_to_parameter`. Emit warning, exit 1 from the assignment,
    //      don't propagate fatally; bash matches.
    //   3. Silent identity (existence checks) — `[[ -v ref ]]`,
    //      `unset ref[N]`. Use `resolve_nameref_or_self_on_cycle` below.
    //   4. Propagate (declarations) — `declare -x ref`, where bash exits
    //      non-zero from the declaration itself. Use `resolve_nameref()?`.
    // Builtins with custom diagnostic formatting (e.g., export) handle the
    // cycle error inline.
    //

    /// Resolves a nameref, falling back to an identity `ResolvedName` **only**
    /// for `CircularNameReference` errors. Other errors propagate.
    pub fn resolve_nameref_or_self_on_cycle(
        &self,
        name: &str,
    ) -> Result<ResolvedName, error::Error> {
        match self.resolve_nameref(name) {
            Ok(r) => Ok(r),
            Err(e) if matches!(e.kind(), error::ErrorKind::CircularNameReference(_)) => {
                Ok(ResolvedName::plain(name))
            }
            Err(e) => Err(e),
        }
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
        let mut visible_vars: HashMap<&String, &ShellVariable> =
            HashMap::with_capacity(self.entry_count);

        let mut local_count = 0;
        for (scope_type, var_map) in self.scopes.iter().rev() {
            if matches!(scope_type, EnvironmentScope::Local) {
                local_count += 1;
            }

            if lookup_policy.admits(*scope_type, local_count) {
                for (name, var) in var_map.iter() {
                    if let hash_map::Entry::Vacant(entry) = visible_vars.entry(name) {
                        entry.insert(var);
                    }
                }
            }

            if lookup_policy.terminates_after(*scope_type) {
                break;
            }
        }

        visible_vars.into_iter()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
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
    fn resolve_nameref_unparsed_strips_no_subscript() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();
        // Plain target — name should be "target", no subscript handling.
        assert_eq!(env.resolve_nameref_unparsed("ref").unwrap(), "target");
    }

    #[test]
    fn resolve_nameref_unparsed_preserves_subscript() {
        // resolve_nameref_unparsed returns the resolved string AS-IS, without
        // parsing any subscript out of it. This is important for `[[ -v ref ]]`
        // semantics where bash treats "arr[2]" literally as a variable name.
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref("arr[2]"), EnvironmentScope::Global)
            .unwrap();
        assert_eq!(env.resolve_nameref_unparsed("ref").unwrap(), "arr[2]");
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
    // resolve_nameref_or_self_on_cycle
    //

    #[test]
    fn resolve_nameref_or_self_on_cycle_returns_resolved_on_success() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref_or_self_on_cycle("ref").unwrap();
        assert_eq!(r.name(), "target");
    }

    #[test]
    fn resolve_nameref_or_self_on_cycle_returns_identity_on_circular() {
        let mut env = ShellEnvironment::new();
        env.add("a", make_nameref("b"), EnvironmentScope::Global)
            .unwrap();
        env.add("b", make_nameref("a"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref_or_self_on_cycle("a").unwrap();
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

        let resolved = ResolvedName::plain("target");
        let (scope, var) = env
            .lookup_resolved(&resolved)
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
        assert!(
            env.lookup("x")
                .bypassing_nameref()
                .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
                .get()
                .is_none()
        );
        // But it IS visible with Anywhere.
        assert!(
            env.lookup("x")
                .bypassing_nameref()
                .in_scope(EnvironmentLookup::Anywhere)
                .get()
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
        let resolved = env.lookup_mut("ref").get().expect("should find target");
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
        assert!(
            env.lookup("nonexistent")
                .bypassing_nameref()
                .get()
                .is_none()
        );
        let resolved = ResolvedName::plain("nonexistent");
        assert!(env.lookup_resolved(&resolved).get().is_none());
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

        let resolved = ResolvedName::plain("x");

        // OnlyInGlobal should find the global one.
        let (scope, var) = env
            .lookup_resolved(&resolved)
            .in_scope(EnvironmentLookup::OnlyInGlobal)
            .get()
            .expect("should find global x");
        assert_eq!(scope, EnvironmentScope::Global);
        assert_eq!(var_str(var), "global");

        // OnlyInCurrentLocal should find the local one.
        let (scope, var) = env
            .lookup_resolved(&resolved)
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get()
            .expect("should find local x");
        assert_eq!(scope, EnvironmentScope::Local);
        assert_eq!(var_str(var), "local");

        env.pop_scope(EnvironmentScope::Local).unwrap();
    }
}
