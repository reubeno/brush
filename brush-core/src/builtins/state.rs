//! State a builtin keeps between invocations, which follows the shell's
//! subshell rules.

use std::any::{Any, TypeId};
use std::collections::HashMap;

/// A builtin's state, as the shell stores it: any `Clone + Send + Sync` value.
///
/// The shell clones it when it clones itself (for a subshell, a pipeline
/// stage, or a command substitution), so a change made in a subshell stays
/// there, as a change to a variable does.
///
/// `Box<dyn AnyState>` satisfies the blanket implementation itself, so the
/// shell calls these through an explicit dereference of the box.
pub(crate) trait AnyState: Any + Send + Sync {
    /// Clones the state, boxed.
    fn clone_box(&self) -> Box<dyn AnyState>;
    /// The state, for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// The state, for downcasting mutably.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any + Clone + Send + Sync> AnyState for T {
    fn clone_box(&self) -> Box<dyn AnyState> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Clone for Box<dyn AnyState> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

/// Every builtin's state, keyed by the state's type alone: the type is the
/// state's identity, whatever name the builtin that uses it was invoked under.
/// So one type registered under several names (`declare` and `typeset`) keeps
/// one state, and related builtins share one by sharing its type.
#[derive(Clone, Default)]
pub(crate) struct BuiltinStates {
    states: HashMap<TypeId, Box<dyn AnyState>>,
}

impl BuiltinStates {
    /// Returns the state of type `S`, if a builtin has used it.
    pub(crate) fn get<S: 'static>(&self) -> Option<&S> {
        let state = self.states.get(&TypeId::of::<S>())?;
        (**state).as_any().downcast_ref()
    }

    /// Sets the state of type `S`, replacing any there was.
    pub(crate) fn set<S: Clone + Send + Sync + 'static>(&mut self, state: S) {
        self.states.insert(TypeId::of::<S>(), Box::new(state));
    }

    /// Returns the state of type `S`, seeding it with `S::default()` on first
    /// use.
    #[allow(
        clippy::unwrap_used,
        reason = "the state under a type's id is of that type"
    )]
    pub(crate) fn get_mut_or_default<S: Clone + Default + Send + Sync + 'static>(
        &mut self,
    ) -> &mut S {
        let state = self
            .states
            .entry(TypeId::of::<S>())
            .or_insert_with(|| Box::new(S::default()));
        (**state).as_any_mut().downcast_mut().unwrap()
    }
}
