//! Implements a shell variable environment.
//!
//! Reads go through [`ShellEnvironment::lookup`] (auto-resolving) or
//! [`ShellEnvironment::lookup_resolved`] (pre-resolved). Mutations go through the
//! [`ShellEnvironment::write`] builder / [`ShellEnvironment::unset`] — or
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

pub use lookup::{DirectVarLookup, DirectVarLookupMut, ResolvedVarRef, VarLookup};
pub use mutation::VarWrite;
pub use names::{
    BaseRef, NameRef, NameRefFault, ResolvedName, ResolvedScope, valid_nameref_target_name,
    valid_variable_name,
};
pub(crate) use names::UnparsedNameRef;
pub(crate) use scope::ScopeGuard;
pub use scope::{EnvironmentLookup, EnvironmentScope};
pub(crate) use var_map::ShellVariableMap;

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
    /// Total number of entries across all scope maps. A variable shadowed in
    /// several scopes is counted once per scope, and unset-local placeholders
    /// count too — so this is an upper bound on the number of *distinct*
    /// visible variables, used only as a capacity hint for the dedup maps built
    /// in the `iter*` methods. Kept in sync by `add` (+1 on a fresh insert),
    /// `unset_raw` (−1 when an entry is truly removed), and `pop_scope` (−N for
    /// the dropped scope's entries).
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
            Some((actual_scope_type, map)) if actual_scope_type == expected_scope_type => {
                // Keep entry_count in sync: the dropped scope's entries (real
                // vars and unset placeholders alike) are gone. Without this the
                // count grew without bound across every `local`-using call.
                self.entry_count = self.entry_count.saturating_sub(map.len());
                Ok(())
            }
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
    // All three resolving entry points walk the same chain and differ only in
    // the final shape, and they all surface a [`NameRefFault`] (cycle or
    // max-depth) as the `Err` arm — a *dedicated* error type, not a variant of
    // the catch-all `Error`. That is deliberate: every caller is forced to
    // choose a recovery policy at the call site, so the cycle case can't be
    // silently dropped or handled inconsistently.
    //   resolve_nameref          → ResolvedName, parses "arr[2]" into base + subscript
    //   resolve_nameref_unparsed → String, returns the final string as-is (used
    //                              for `[[ -v ref ]]` where bash takes the
    //                              resolved target as a literal variable name)
    //   resolve_nameref_or_self  → ResolvedName, infallible — silent identity on
    //                              any fault (for existence checks / element unset)
    //   resolve_nameref_chain    → Cow<str>, lowest-level (private)
    //
    // The remaining recovery policies (warn+identity, warn+skip, propagate)
    // live at the call sites because emitting a warning needs stderr access the
    // environment doesn't have; the call site `match`es the fault and calls
    // `Shell::warn_nameref_fault` / `WordExpander::warn_nameref_fault`.
    //
    // For *lookups* that resolve namerefs and want subscript-aware access,
    // prefer the lookup builders in `lookup.rs` (`lookup` / `lookup_resolved`).
    //

    /// Resolves a nameref chain, returning `(final target string, scope)`. The
    /// [`ResolvedScope`] is [`Global`](ResolvedScope::Global) when a
    /// *self-referential* nameref (`x → "x"`) at a function-local scope is hit:
    /// bash resolves such a nameref's target against the global scope (so
    /// `local -n x=x` targets the global `x`), and callers must look the result
    /// up there. On a cycle or depth overflow, returns a [`NameRefFault`] that
    /// blames `name` (the head of the chain), matching bash's diagnostics.
    fn resolve_nameref_chain<'a>(
        &'a self,
        name: &'a str,
    ) -> Result<(Cow<'a, str>, ResolvedScope), NameRefFault> {
        self.resolve_nameref_chain_using_policy(name, EnvironmentLookup::Anywhere)
    }

    fn resolve_nameref_chain_using_policy<'a>(
        &'a self,
        name: &'a str,
        lookup_policy: EnvironmentLookup,
    ) -> Result<(Cow<'a, str>, ResolvedScope), NameRefFault> {
        // Quick check: is this even a nameref?
        let (head_scope, first_target) =
            match self.get_by_exact_name_using_policy(name, lookup_policy) {
                Some((scope, var)) if var.is_treated_as_nameref() => match var.value() {
                    ShellValue::String(s) if !s.is_empty() => (scope, s.as_str()),
                    _ => return Ok((Cow::Borrowed(name), ResolvedScope::Default)),
                },
                _ => return Ok((Cow::Borrowed(name), ResolvedScope::Default)),
            };

        // A self-referential nameref (`x → "x"`) is special: bash resolves it
        // against the GLOBAL scope. A function-local `local -n x=x` therefore
        // refers to the global `x` (skipping its own and intermediate locals).
        // At global scope there is no enclosing scope to fall back to, so it's a
        // true cycle.
        if first_target == name {
            return Self::resolve_self_reference(name, name, head_scope);
        }

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
                // bash blames the head of the chain (`name`), not the node where
                // the cycle was detected.
                return Err(NameRefFault::circular(name));
            }

            // N.B. When `current` is a subscripted target like "arr[2]",
            // `get_by_exact_name` does a literal HashMap lookup for "arr[2]"
            // which won't match any variable — correctly terminating the chain.
            // The subscript is parsed later by `resolve_nameref` /
            // `parse_nameref_subscript`. Do NOT "fix" this to parse subscripts
            // here; that would cause double resolution when callers use
            // resolve_nameref().
            let (scope, target) = match self.get_by_exact_name_using_policy(current, lookup_policy)
            {
                Some((scope, var)) if var.is_treated_as_nameref() => match var.value() {
                    ShellValue::String(s) if !s.is_empty() => (scope, s.as_str()),
                    _ => return Ok((Cow::Borrowed(current), ResolvedScope::Default)),
                },
                _ => return Ok((Cow::Borrowed(current), ResolvedScope::Default)),
            };

            // Self-reference mid-chain (e.g. `a → b`, `b → "b"`).
            if target == current {
                return Self::resolve_self_reference(name, current, scope);
            }

            visited.push(current);
            // Check depth *after* following this link, matching bash's
            // NAMEREF_MAX which counts resolution steps, not chain length.
            if visited.len() > MAX_NAMEREF_DEPTH {
                return Err(NameRefFault::max_depth(name, MAX_NAMEREF_DEPTH));
            }
            current = target;
        }
    }

    /// Resolves a self-referential nameref whose value equals its own name.
    /// `blame` is the chain head (named in a fault); `self_name` is the
    /// self-referencing variable (the one to resolve at global scope); `scope`
    /// is the scope `self_name` was found in. At a non-global scope the result
    /// resolves `self_name` against the global scope ([`ResolvedScope::Global`]);
    /// at global scope there's nothing to fall back to, so it's a true cycle.
    fn resolve_self_reference<'a>(
        blame: &str,
        self_name: &'a str,
        scope: EnvironmentScope,
    ) -> Result<(Cow<'a, str>, ResolvedScope), NameRefFault> {
        if matches!(scope, EnvironmentScope::Global) {
            Err(NameRefFault::circular(blame))
        } else {
            Ok((Cow::Borrowed(self_name), ResolvedScope::Global))
        }
    }

    /// Resolves a nameref chain and parses any subscript from the final target.
    /// `ref→"arr[2]"` returns `ResolvedName { name: "arr", subscript: Some("2") }`.
    ///
    /// A self-referential function-local nameref (`local -n x=x`) resolves to
    /// `x` with [`ResolvedScope::Global`] — see [`ResolvedName::resolved_scope`].
    ///
    /// The returned [`ResolvedName`] models only named/subscripted targets, not
    /// positional parameters (`declare -n ref=1`, which bash also rejects).
    pub fn resolve_nameref(&self, name: &str) -> Result<ResolvedName, NameRefFault> {
        let (resolved, scope) = self.resolve_nameref_chain(name)?;
        Ok(ResolvedName::parse(resolved.into_owned()).with_scope(scope))
    }

    /// Resolves a nameref chain using `lookup_policy` for each nameref lookup.
    ///
    /// This is primarily for declaration builtins with an explicit scope policy
    /// such as `declare -g`, where local namerefs must not redirect a global
    /// declaration.
    #[doc(hidden)]
    pub fn resolve_nameref_using_policy(
        &self,
        name: &str,
        lookup_policy: EnvironmentLookup,
    ) -> Result<ResolvedName, NameRefFault> {
        let (resolved, scope) = self.resolve_nameref_chain_using_policy(name, lookup_policy)?;
        Ok(ResolvedName::parse(resolved.into_owned()).with_scope(scope))
    }

    /// Resolves a nameref chain, returning `(final target string, scope)`
    /// verbatim (no subscript parsing). For `[[ -v ref ]]` semantics, where bash
    /// treats `arr[2]` as a literal variable name. The [`ResolvedScope`] is
    /// [`Global`](ResolvedScope::Global) for a self-referential function-local
    /// nameref (`local -n x=x`), whose existence must be checked globally.
    pub(crate) fn resolve_nameref_unparsed(
        &self,
        name: &str,
    ) -> Result<UnparsedNameRef, NameRefFault> {
        let (resolved, scope) = self.resolve_nameref_chain(name)?;
        Ok(UnparsedNameRef {
            name: resolved.into_owned(),
            scope,
        })
    }

    /// Resolves a nameref, falling back to an identity `ResolvedName` on **any**
    /// fault (cycle or depth overflow), silently. For existence checks
    /// (`[[ -v ref ]]`) and element unset (`unset ref[N]`), where bash treats an
    /// unresolvable nameref as the (absent) literal variable.
    pub fn resolve_nameref_or_self(&self, name: &str) -> ResolvedName {
        self.resolve_nameref(name)
            .unwrap_or_else(|_| ResolvedName::plain(name))
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
        assert!(err.is_circular());
        // bash blames the head of the chain.
        assert_eq!(err.head(), "self");
    }

    #[test]
    fn resolve_nameref_two_node_cycle_is_circular() {
        let mut env = ShellEnvironment::new();
        env.add("a", make_nameref("b"), EnvironmentScope::Global)
            .unwrap();
        env.add("b", make_nameref("a"), EnvironmentScope::Global)
            .unwrap();
        let err = env.resolve_nameref("a").unwrap_err();
        assert!(err.is_circular());
        assert_eq!(err.head(), "a");
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
        assert!(err.is_circular());
        assert_eq!(err.head(), "c1");
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
        // A too-deep but acyclic chain reports a max-depth fault, NOT a cycle.
        assert!(!err.is_circular());
        assert_eq!(err.head(), prev);
    }

    #[test]
    fn resolve_nameref_self_name_local_resolves_to_global() {
        // A function-local `local -n x=x` resolves `x` flagged for global scope.
        let mut env = ShellEnvironment::new();
        env.add("x", make_var("global"), EnvironmentScope::Global)
            .unwrap();
        env.push_scope(EnvironmentScope::Local);
        env.add("x", make_nameref("x"), EnvironmentScope::Local)
            .unwrap();

        let r = env.resolve_nameref("x").unwrap();
        assert_eq!(r.name(), "x");
        assert!(r.is_global_scope());
        // The auto-resolving lookup follows it to the GLOBAL `x`, not the local
        // nameref.
        let resolved = env.lookup("x").get().expect("should find global x");
        assert_eq!(resolved.scope(), EnvironmentScope::Global);
        assert_eq!(var_str(resolved.base_var()), "global");

        env.pop_scope(EnvironmentScope::Local).unwrap();
    }

    #[test]
    fn resolve_nameref_self_name_global_is_circular() {
        // At global scope there's no enclosing scope to fall back to.
        let mut env = ShellEnvironment::new();
        env.add("x", make_nameref("x"), EnvironmentScope::Global)
            .unwrap();
        let err = env.resolve_nameref("x").unwrap_err();
        assert!(err.is_circular());
    }

    #[test]
    fn resolve_nameref_unparsed_strips_no_subscript() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();
        // Plain target — name should be "target", no subscript handling, and
        // not global-scoped (no self-reference).
        let r = env.resolve_nameref_unparsed("ref").unwrap();
        assert_eq!(r.name(), "target");
        assert_eq!(r.resolved_scope(), ResolvedScope::Default);
    }

    #[test]
    fn resolve_nameref_unparsed_preserves_subscript() {
        // resolve_nameref_unparsed returns the resolved string AS-IS, without
        // parsing any subscript out of it. This is important for `[[ -v ref ]]`
        // semantics where bash treats "arr[2]" literally as a variable name.
        let mut env = ShellEnvironment::new();
        env.add("ref", make_nameref("arr[2]"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref_unparsed("ref").unwrap();
        assert_eq!(r.name(), "arr[2]");
        assert_eq!(r.resolved_scope(), ResolvedScope::Default);
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
    // resolve_nameref_or_self
    //

    #[test]
    fn resolve_nameref_or_self_returns_resolved_on_success() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("v"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref_or_self("ref");
        assert_eq!(r.name(), "target");
    }

    #[test]
    fn resolve_nameref_or_self_returns_identity_on_circular() {
        let mut env = ShellEnvironment::new();
        env.add("a", make_nameref("b"), EnvironmentScope::Global)
            .unwrap();
        env.add("b", make_nameref("a"), EnvironmentScope::Global)
            .unwrap();
        let r = env.resolve_nameref_or_self("a");
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
        assert!(resolved.subscript().is_none());
    }

    #[test]
    fn lookup_str_bypassing_nameref_returns_nameref_itself() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();
        env.add("ref", make_nameref("target"), EnvironmentScope::Global)
            .unwrap();

        // lookup("ref").bypassing_nameref().get() should return the nameref variable.
        let r = env
            .lookup("ref")
            .bypassing_nameref()
            .get()
            .expect("should find ref");
        assert_eq!(r.scope(), EnvironmentScope::Global);
        assert!(r.base_var().is_treated_as_nameref());
        assert_eq!(var_str(r.base_var()), "target");
    }

    #[test]
    fn lookup_resolved_name_skips_resolution() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("hello"), EnvironmentScope::Global)
            .unwrap();

        let resolved = ResolvedName::plain("target");
        let r = env
            .lookup_resolved(resolved.base())
            .get()
            .expect("should find target");
        assert_eq!(r.scope(), EnvironmentScope::Global);
        assert_eq!(var_str(r.base_var()), "hello");
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

        let r = env
            .lookup("x")
            .bypassing_nameref()
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get()
            .expect("should find local x");
        assert_eq!(r.scope(), EnvironmentScope::Local);
        assert_eq!(var_str(r.base_var()), "local");
        env.pop_scope(EnvironmentScope::Local).unwrap();
    }

    #[test]
    fn entry_count_returns_to_baseline_after_scope_pop() {
        // Regression: entry_count must not grow without bound across
        // push_scope/add/pop_scope cycles (it feeds iter*'s capacity hint).
        let mut env = ShellEnvironment::new();
        let baseline = env.entry_count;
        for _ in 0..5 {
            env.push_scope(EnvironmentScope::Local);
            env.add("a", make_var("1"), EnvironmentScope::Local)
                .unwrap();
            env.add("b", make_var("2"), EnvironmentScope::Local)
                .unwrap();
            env.pop_scope(EnvironmentScope::Local).unwrap();
        }
        assert_eq!(env.entry_count, baseline);
    }

    #[test]
    fn add_rejects_invalid_variable_names() {
        let mut env = ShellEnvironment::new();
        assert!(
            env.add("arr[0]", make_var("value"), EnvironmentScope::Global)
                .is_err()
        );
        assert!(
            env.add("1bad", make_var("value"), EnvironmentScope::Global)
                .is_err()
        );
    }

    #[test]
    fn lookup_mut_resolved_finds_and_allows_mutation() {
        let mut env = ShellEnvironment::new();
        env.add("target", make_var("original"), EnvironmentScope::Global)
            .unwrap();

        // Pre-resolved mutable lookup is the supported mutation path.
        let resolved = ResolvedName::plain("target");
        let (scope, var) = env
            .lookup_mut_resolved(resolved.base())
            .get()
            .expect("should find target");
        assert_eq!(scope, EnvironmentScope::Global);
        assert!(!var.is_treated_as_nameref());
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
        assert!(env.lookup_resolved(resolved.base()).get().is_none());
    }

    #[test]
    fn lookup_str_auto_resolve_with_subscripted_nameref() {
        let mut env = ShellEnvironment::new();
        let arr = ShellVariable::new(ShellValue::indexed_array(["zero", "one", "two"]));
        env.add("arr", arr, EnvironmentScope::Global).unwrap();
        env.add("ref", make_nameref("arr[1]"), EnvironmentScope::Global)
            .unwrap();

        let resolved = env.lookup("ref").get().expect("should find arr");
        assert!(resolved.subscript().is_some());
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
        let r = env
            .lookup_resolved(resolved.base())
            .in_scope(EnvironmentLookup::OnlyInGlobal)
            .get()
            .expect("should find global x");
        assert_eq!(r.scope(), EnvironmentScope::Global);
        assert_eq!(var_str(r.base_var()), "global");

        // OnlyInCurrentLocal should find the local one.
        let r = env
            .lookup_resolved(resolved.base())
            .in_scope(EnvironmentLookup::OnlyInCurrentLocal)
            .get()
            .expect("should find local x");
        assert_eq!(r.scope(), EnvironmentScope::Local);
        assert_eq!(var_str(r.base_var()), "local");

        env.pop_scope(EnvironmentScope::Local).unwrap();
    }
}
