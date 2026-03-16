//! Scope types and scope guards for the shell environment.

use crate::extensions;

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

impl EnvironmentLookup {
    /// Returns `true` if the given scope passes this policy's filter.
    ///
    /// `local_count` is the number of local frames visited so far (including
    /// the current one) when walking the stack top-down — used to identify
    /// the "current" local scope for `OnlyInCurrentLocal`.
    pub(super) const fn admits(self, scope: EnvironmentScope, local_count: usize) -> bool {
        match self {
            Self::Anywhere => true,
            Self::OnlyInGlobal => matches!(scope, EnvironmentScope::Global),
            Self::OnlyInCurrentLocal => {
                matches!(scope, EnvironmentScope::Local) && local_count == 1
            }
            Self::OnlyInLocal => matches!(scope, EnvironmentScope::Local),
        }
    }

    /// Returns `true` if iteration should stop after visiting this scope.
    ///
    /// `OnlyInCurrentLocal` terminates after the first local frame regardless
    /// of match; all others keep walking until the stack is exhausted.
    pub(super) const fn terminates_after(self, scope: EnvironmentScope) -> bool {
        matches!(self, Self::OnlyInCurrentLocal) && matches!(scope, EnvironmentScope::Local)
    }
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
