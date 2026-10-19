//! The builtins registered with a shell.

use std::collections::HashMap;

use super::Registration;
use crate::extensions;

/// The builtins registered with a shell, by name; see
/// [`Shell::builtins`](crate::Shell::builtins) and
/// [`Shell::builtins_mut`](crate::Shell::builtins_mut).
///
/// How it stores them is private, so that can change without breaking
/// anyone. A clone (as a subshell's) is independent of the registry it was
/// cloned from: registering, enabling or disabling a builtin in one doesn't
/// change the other.
#[derive(Clone, Default)]
pub struct Registry<SE: extensions::ShellExtensions> {
    registrations: HashMap<String, Registration<SE>>,
}

impl<SE: extensions::ShellExtensions> Registry<SE> {
    /// Returns the builtin registered under `name`, if any. It may be disabled
    /// (see [`Registration::is_enabled`]).
    ///
    /// # Arguments
    ///
    /// * `name` - The builtin's name.
    pub fn get(&self, name: &str) -> Option<&Registration<SE>> {
        self.registrations.get(name)
    }

    /// Returns the builtin registered under `name`, if any, to change it (to
    /// enable or disable it, say).
    ///
    /// # Arguments
    ///
    /// * `name` - The builtin's name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Registration<SE>> {
        self.registrations.get_mut(name)
    }

    /// Registers a builtin under `name`, replacing any registered under it.
    ///
    /// The registration replaced is gone with all it said about the builtin:
    /// a special builtin replaced by a registration made without
    /// [`Registration::special`] is no longer special.
    ///
    /// # Arguments
    ///
    /// * `name` - The builtin's name.
    /// * `registration` - The builtin's registration.
    pub fn register(&mut self, name: impl Into<String>, registration: Registration<SE>) {
        self.registrations.insert(name.into(), registration);
    }

    /// Registers a builtin under `name` unless one is registered under it
    /// already, which is left as it is.
    ///
    /// # Arguments
    ///
    /// * `name` - The builtin's name.
    /// * `registration` - The builtin's registration.
    pub fn register_if_absent(&mut self, name: impl Into<String>, registration: Registration<SE>) {
        self.registrations
            .entry(name.into())
            .or_insert(registration);
    }

    /// Returns the registered builtins with their names, enabled or not, in
    /// order of name (by byte value, so `.`, `:` and `[` come before letters).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Registration<SE>)> {
        // Stored unordered, for fast lookup by name; listing them is rare
        // enough that sorting each time costs nothing that matters.
        let mut entries: Vec<_> = self
            .registrations
            .iter()
            .map(|(name, registration)| (name.as_str(), registration))
            .collect();
        entries.sort_unstable_by_key(|(name, _)| *name);
        entries.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::{
        Builtin, ContentOptions, ContentType, Invocation, ParseArgs, UsageError,
    };
    use crate::{ExecutionContext, ExecutionResult, extensions::DefaultShellExtensions};

    /// A builtin that does nothing; registrations of it differ only in what
    /// they say about it.
    struct Nothing;

    impl ParseArgs for Nothing {
        fn content(
            name: &str,
            _content_type: ContentType,
            _options: &ContentOptions,
        ) -> Result<String, crate::Error> {
            Ok(format!("{name}\n"))
        }

        fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
            Ok(Self)
        }
    }

    impl<SE: extensions::ShellExtensions> Builtin<SE> for Nothing {
        type Error = crate::Error;

        #[allow(
            clippy::unused_async_trait_impl,
            reason = "the trait's method is async; this one has nothing to wait for"
        )]
        async fn execute(
            self,
            _context: ExecutionContext<'_, SE>,
        ) -> Result<ExecutionResult, Self::Error> {
            Ok(ExecutionResult::success())
        }
    }

    type TestRegistry = Registry<DefaultShellExtensions>;

    fn plain() -> Registration<DefaultShellExtensions> {
        Registration::of::<Nothing>()
    }

    fn special() -> Registration<DefaultShellExtensions> {
        Registration::of::<Nothing>().special()
    }

    fn names(registry: &TestRegistry) -> Vec<&str> {
        registry.iter().map(|(name, _)| name).collect()
    }

    #[test]
    fn an_empty_registry_finds_nothing() {
        let mut registry = TestRegistry::default();
        assert!(registry.get("x").is_none());
        assert!(registry.get_mut("x").is_none());
        assert_eq!(registry.iter().count(), 0);
    }

    #[test]
    fn a_registered_builtin_is_found_by_its_name_alone() {
        let mut registry = TestRegistry::default();
        registry.register("x", special());
        assert!(registry.get("x").is_some_and(Registration::is_special));
        assert!(registry.get("X").is_none());
        assert!(registry.get("x ").is_none());
    }

    #[test]
    fn register_replaces_what_was_registered() {
        let mut registry = TestRegistry::default();
        registry.register("x", special());
        registry.register("x", plain());
        assert!(registry.get("x").is_some_and(|r| !r.is_special()));
        assert_eq!(names(&registry), ["x"]);
    }

    #[test]
    fn register_if_absent_keeps_what_was_registered() {
        let mut registry = TestRegistry::default();
        registry.register_if_absent("x", special());
        assert!(registry.get("x").is_some_and(Registration::is_special));

        registry.register_if_absent("x", plain());
        assert!(registry.get("x").is_some_and(Registration::is_special));
        assert_eq!(names(&registry), ["x"]);
    }

    #[test]
    fn a_builtin_is_disabled_and_enabled_through_its_registration() {
        let mut registry = TestRegistry::default();
        registry.register("x", plain());
        registry.register("y", plain());

        registry
            .get_mut("x")
            .into_iter()
            .for_each(|r| r.set_enabled(false));
        assert!(registry.get("x").is_some_and(|r| !r.is_enabled()));
        assert!(registry.get("y").is_some_and(Registration::is_enabled));
        // A disabled builtin is still listed.
        assert_eq!(names(&registry), ["x", "y"]);

        registry
            .get_mut("x")
            .into_iter()
            .for_each(|r| r.set_enabled(true));
        assert!(registry.get("x").is_some_and(Registration::is_enabled));
    }

    #[test]
    fn builtins_are_listed_in_order_of_name() {
        let mut registry = TestRegistry::default();
        for name in ["wait", "[", "alias", ":", "echo", ".", "Z", "a"] {
            registry.register(name, plain());
        }
        assert_eq!(
            names(&registry),
            [".", ":", "Z", "[", "a", "alias", "echo", "wait"]
        );
    }

    #[test]
    fn a_clone_changes_independently() {
        let mut registry = TestRegistry::default();
        registry.register("x", plain());

        let mut copy = registry.clone();
        copy.get_mut("x")
            .into_iter()
            .for_each(|r| r.set_enabled(false));
        copy.register("y", plain());

        assert!(registry.get("x").is_some_and(Registration::is_enabled));
        assert_eq!(names(&registry), ["x"]);
        assert_eq!(names(&copy), ["x", "y"]);
    }
}
