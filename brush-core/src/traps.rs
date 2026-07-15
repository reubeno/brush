//! Facilities for configuring trap handlers.

use std::str::FromStr;
use std::{collections::HashMap, fmt::Display};

use itertools::Itertools as _;

use crate::{error, sys};

/// Type of signal that can be trapped in the shell.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TrapSignal {
    /// A system signal.
    Signal(sys::signal::Signal),
    /// The `DEBUG` trap.
    Debug,
    /// The `ERR` trap.
    Err,
    /// The `EXIT` trap.
    Exit,
    /// The `RETURN` trp.
    Return,
}

#[cfg(feature = "serde")]
impl serde::Serialize for TrapSignal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TrapSignal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl Display for TrapSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TrapSignal {
    /// Returns all possible values of [`TrapSignal`].
    pub fn iterator() -> impl Iterator<Item = Self> {
        const SIGNALS: &[TrapSignal] = &[TrapSignal::Debug, TrapSignal::Err, TrapSignal::Exit];

        let iter = itertools::chain!(
            SIGNALS.iter().copied(),
            sys::signal::Signal::iterator().map(TrapSignal::Signal)
        );

        iter
    }

    /// Converts [`TrapSignal`] into its corresponding signal name as a [`&'static str`](str)
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Signal(s) => s.as_str(),
            Self::Debug => "DEBUG",
            Self::Err => "ERR",
            Self::Exit => "EXIT",
            Self::Return => "RETURN",
        }
    }
}

/// Formats [`Iterator<Item = TrapSignal>`](TrapSignal)  to the provided writer.
///
/// # Arguments
///
/// * `f` - Any type that implements [`std::io::Write`].
/// * `it` - An iterator over the signals that will be formatted into the `f`.
pub fn format_signals(
    mut f: impl std::io::Write,
    it: impl Iterator<Item = TrapSignal>,
) -> Result<(), error::Error> {
    let it = it
        .filter_map(|s| i32::try_from(s).ok().map(|n| (s, n)))
        .sorted_by(|a, b| Ord::cmp(&a.1, &b.1))
        .format_with("\n", |s, f| f(&format_args!("{}) {}", s.1, s.0)));
    write!(f, "{it}")?;
    Ok(())
}

// implement s.parse::<TrapSignal>()
impl FromStr for TrapSignal {
    type Err = error::Error;
    fn from_str(s: &str) -> Result<Self, <Self as FromStr>::Err> {
        if let Ok(n) = s.parse::<i32>() {
            Self::try_from(n)
        } else {
            Self::try_from(s)
        }
    }
}

// from a signal number
impl TryFrom<i32> for TrapSignal {
    type Error = error::Error;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        // NOTE: DEBUG and ERR are real-time signals, defined based on NSIG or SIGRTMAX (is not
        // available on bsd-like systems),
        // and don't have persistent numbers across platforms, so we skip them here.
        Ok(match value {
            0 => Self::Exit,
            value => Self::Signal(
                sys::signal::Signal::try_from(value)
                    .map_err(|_| error::ErrorKind::InvalidSignal(value.to_string()))?,
            ),
        })
    }
}

// from a signal name
impl TryFrom<&str> for TrapSignal {
    type Error = error::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        #[allow(unused_mut, reason = "only mutated on some platforms")]
        let mut s = value.to_ascii_uppercase();

        Ok(match s.as_str() {
            "DEBUG" => Self::Debug,
            "ERR" => Self::Err,
            "EXIT" => Self::Exit,
            "RETURN" => Self::Return,
            _ => {
                // Bash compatibility:
                // support for signal names without the `SIG` prefix, for example `HUP` -> `SIGHUP`
                if !s.starts_with("SIG") {
                    s.insert_str(0, "SIG");
                }
                sys::signal::Signal::from_str(s.as_str())
                    .map(TrapSignal::Signal)
                    .map_err(|_| error::ErrorKind::InvalidSignal(value.into()))?
            }
        })
    }
}

/// Error type used when failing to convert a `TrapSignal` to a number.
#[derive(Debug, Clone, Copy)]
pub struct TrapSignalNumberError;

impl TryFrom<TrapSignal> for i32 {
    type Error = TrapSignalNumberError;
    fn try_from(value: TrapSignal) -> Result<Self, Self::Error> {
        Ok(match value {
            TrapSignal::Signal(s) => s as Self,
            TrapSignal::Exit => 0,
            _ => return Err(TrapSignalNumberError),
        })
    }
}

impl TrapSignal {
    /// Returns whether handlers for this signal remain live within shell
    /// functions under the given options (i.e., are not suspended at function
    /// entry). Per bash, the `ERR` trap is inherited only with errtrace
    /// (`set -E`), and the `DEBUG`/`RETURN` traps only with functrace
    /// (`set -T`); `extdebug` implies both. `EXIT` and system-signal traps are
    /// never suspended by function calls. (Child *shells* additionally reset
    /// `EXIT`/signal dispositions; see [`TrapHandlerConfig::child_copy`], which
    /// owns that policy.)
    pub(crate) const fn inherited_by_functions(
        self,
        options: &crate::options::RuntimeOptions,
    ) -> bool {
        match self {
            Self::Err => options.shell_functions_inherit_err_trap || options.enable_debugger,
            Self::Debug | Self::Return => {
                options.shell_functions_inherit_debug_and_return_traps || options.enable_debugger
            }
            Self::Exit | Self::Signal(_) => true,
        }
    }
}

/// A handler for a trap signal.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrapHandler {
    /// The source text of the command to invoke.
    pub command: String,
    /// Source information for where the trap handler was defined.
    pub source_info: crate::SourceInfo,
}

/// The disposition in effect for a trapped signal.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum TrapDisposition {
    /// A handler executes when the trap fires.
    Handle(TrapHandler),
    /// The signal is ignored (`trap '' SIG`, i.e. an empty command). Per bash,
    /// ignore dispositions are real state rather than handlers: they're
    /// inherited by child shells unconditionally and are not suspended at
    /// function entry.
    Ignore(TrapHandler),
    /// Retained from a parent shell solely so `trap -p` can display it; never
    /// executed. Per bash, child shells reset trap dispositions the child
    /// doesn't inherit, but keep their command strings displayable until the
    /// child modifies any trap.
    DisplayOnly(TrapHandler),
}

impl TrapDisposition {
    /// Returns the handler recorded for this disposition (for display, and for
    /// execution when live).
    const fn handler(&self) -> &TrapHandler {
        match self {
            Self::Handle(handler) | Self::Ignore(handler) | Self::DisplayOnly(handler) => handler,
        }
    }
}

/// Configuration for trap handlers in the shell.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrapHandlerConfig {
    /// Registered dispositions for traps, keyed by signal type.
    handlers: HashMap<TrapSignal, TrapDisposition>,
    /// Stack of suspension scopes. Traps that aren't inherited by shell functions
    /// (e.g. `ERR` without errtrace, `DEBUG`/`RETURN` without functrace) are moved
    /// here for the duration of a function call: while suspended they're neither
    /// visible nor executed, but they're reinstated when the scope is popped unless
    /// a replacement handler was registered in the interim. (Only handled — not
    /// ignore or display-only — dispositions are ever suspended.)
    #[cfg_attr(feature = "serde", serde(default))]
    suspended_scopes: Vec<Vec<(TrapSignal, TrapHandler)>>,
}

impl TrapHandlerConfig {
    /// Iterates over the registered handlers for trap signals (including those
    /// retained for display purposes only).
    pub fn iter_handlers(&self) -> impl Iterator<Item = (TrapSignal, &TrapHandler)> {
        self.handlers
            .iter()
            .map(|(signal, disposition)| (*signal, disposition.handler()))
    }

    /// Tries to find the handler associated with the given signal (including
    /// handlers retained for display purposes only; use [`Self::live_handler`]
    /// for one that would actually run).
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to get the handler for.
    pub fn get_handler(&self, signal_type: TrapSignal) -> Option<&TrapHandler> {
        self.handlers
            .get(&signal_type)
            .map(TrapDisposition::handler)
    }

    /// Tries to find the handler associated with the given signal that would
    /// actually execute (i.e., excluding display-only handlers).
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to get the handler for.
    pub(crate) fn live_handler(&self, signal_type: TrapSignal) -> Option<&TrapHandler> {
        match self.handlers.get(&signal_type)? {
            TrapDisposition::Handle(handler) | TrapDisposition::Ignore(handler) => Some(handler),
            TrapDisposition::DisplayOnly(_) => None,
        }
    }

    /// Returns whether a handler is registered for the given signal. Note that
    /// this includes handlers retained for display purposes only (which are
    /// never executed); use [`Self::has_live_handler`] to check for a handler
    /// that would actually run.
    pub fn handles(&self, signal_type: TrapSignal) -> bool {
        self.handlers.contains_key(&signal_type)
    }

    /// Returns whether a handler is registered for the given signal that would
    /// actually execute (i.e., excluding display-only handlers).
    pub(crate) fn has_live_handler(&self, signal_type: TrapSignal) -> bool {
        self.live_handler(signal_type).is_some()
    }

    /// Returns the number of suspension scopes currently in effect; kept in
    /// lockstep with the shell's function-call depth.
    pub(crate) const fn suspension_scope_count(&self) -> usize {
        self.suspended_scopes.len()
    }

    /// Registers a handler for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to register a handler for.
    /// * `command` - The command to execute when the signal is trapped.
    /// * `source_info` - The source info for where the trap handler was defined.
    pub fn register_handler(
        &mut self,
        signal_type: TrapSignal,
        command: String,
        source_info: crate::SourceInfo,
    ) {
        self.discard_display_only_handlers();

        let handler = TrapHandler {
            command,
            source_info,
        };
        let disposition = if handler.command.is_empty() {
            TrapDisposition::Ignore(handler)
        } else {
            TrapDisposition::Handle(handler)
        };

        let _ = self.handlers.insert(signal_type, disposition);
    }

    /// Enters a new suspension scope, suspending any currently registered handlers
    /// for the given signals. Suspended handlers are neither visible nor executed
    /// until the scope is exited. Must be paired with a call to
    /// [`Self::exit_suspension_scope`].
    ///
    /// # Arguments
    ///
    /// * `signals` - The signals whose handlers should be suspended in this scope.
    pub(crate) fn enter_suspension_scope(&mut self, signals: impl IntoIterator<Item = TrapSignal>) {
        // Only handled dispositions are suspended. Display-only handlers stay
        // put: they're not live, so there's nothing to suspend, and bash keeps
        // displaying them inside functions in this case. Ignore dispositions
        // (`trap '' SIG`) also stay put: per bash they're state, not handlers,
        // and remain in effect (and displayable) within functions.
        let suspended = signals
            .into_iter()
            .filter_map(|signal| {
                if let std::collections::hash_map::Entry::Occupied(entry) =
                    self.handlers.entry(signal)
                    && matches!(entry.get(), TrapDisposition::Handle(_))
                    && let TrapDisposition::Handle(handler) = entry.remove()
                {
                    Some((signal, handler))
                } else {
                    None
                }
            })
            .collect();
        self.suspended_scopes.push(suspended);
    }

    /// Exits the most recently entered suspension scope, reinstating each handler
    /// suspended by it — unless a replacement handler was registered for that
    /// signal while the scope was active, in which case the replacement persists.
    pub(crate) fn exit_suspension_scope(&mut self) {
        let suspended = self.suspended_scopes.pop();
        debug_assert!(suspended.is_some(), "unbalanced trap suspension scope exit");
        if let Some(suspended) = suspended {
            for (signal, handler) in suspended {
                let _ = self
                    .handlers
                    .entry(signal)
                    .or_insert(TrapDisposition::Handle(handler));
            }
        }
    }

    /// Returns a copy of this configuration representing a child shell instance's
    /// (e.g., a subshell's) view of these traps, resolved with the options in
    /// effect at creation time: handlers the child doesn't inherit are downgraded
    /// to display-only (visible to `trap -p`, never executed), while inherited
    /// ones remain live.
    ///
    /// Suspension scopes are carried over as-is: the child also inherits the
    /// parent's call stack, so the scope stack stays in lockstep with the
    /// function-call depth. The child's own function calls push and pop their
    /// own scopes above the inherited ones, which are never popped within the
    /// child.
    pub(crate) fn child_copy(&self, options: &crate::options::RuntimeOptions) -> Self {
        let mut copy = self.clone();
        for (signal, disposition) in &mut copy.handlers {
            // Ignore dispositions (`trap '' SIG`) are inherited by child shells
            // unconditionally, and display-only ones stay display-only. Handled
            // dispositions are downgraded unless inherited: EXIT and
            // system-signal dispositions are always reset in a child shell, and
            // ERR/DEBUG/RETURN stay live only with errtrace/functrace.
            if let TrapDisposition::Handle(handler) = disposition {
                let live = match signal {
                    TrapSignal::Exit | TrapSignal::Signal(_) => false,
                    signal => signal.inherited_by_functions(options),
                };
                if !live {
                    *disposition = TrapDisposition::DisplayOnly(std::mem::take(handler));
                }
            }
        }
        copy
    }

    /// Removes handlers for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to remove handlers for.
    pub fn remove_handlers(&mut self, signal_type: TrapSignal) {
        self.discard_display_only_handlers();
        self.handlers.remove(&signal_type);
    }

    /// Discards all display-only handlers. Per bash, the first modification of
    /// any trap in a child shell discards the command strings retained from the
    /// parent for display. (Inherited ignore dispositions are unaffected — they
    /// are real state, never display-only.)
    fn discard_display_only_handlers(&mut self) {
        self.handlers
            .retain(|_, disposition| !matches!(disposition, TrapDisposition::DisplayOnly(_)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register(config: &mut TrapHandlerConfig, signal: TrapSignal, command: &str) {
        config.register_handler(signal, command.to_owned(), crate::SourceInfo::default());
    }

    #[test]
    fn suspension_scope_hides_then_reinstates_handler() {
        let mut config = TrapHandlerConfig::default();
        register(&mut config, TrapSignal::Err, "echo err");

        config.enter_suspension_scope([TrapSignal::Err, TrapSignal::Return]);
        assert!(!config.handles(TrapSignal::Err));

        config.exit_suspension_scope();
        assert!(config.handles(TrapSignal::Err));
        assert_eq!(
            config.get_handler(TrapSignal::Err).unwrap().command,
            "echo err"
        );
    }

    #[test]
    fn handler_registered_during_suspension_persists_over_suspended_one() {
        let mut config = TrapHandlerConfig::default();
        register(&mut config, TrapSignal::Err, "echo outer");

        config.enter_suspension_scope([TrapSignal::Err]);
        register(&mut config, TrapSignal::Err, "echo inner");
        assert_eq!(
            config.get_handler(TrapSignal::Err).unwrap().command,
            "echo inner"
        );

        config.exit_suspension_scope();
        assert_eq!(
            config.get_handler(TrapSignal::Err).unwrap().command,
            "echo inner"
        );
    }

    #[test]
    fn suspension_scopes_nest() {
        let mut config = TrapHandlerConfig::default();
        register(&mut config, TrapSignal::Return, "echo outer");

        config.enter_suspension_scope([TrapSignal::Return]);
        register(&mut config, TrapSignal::Return, "echo middle");

        config.enter_suspension_scope([TrapSignal::Return]);
        assert!(!config.handles(TrapSignal::Return));

        config.exit_suspension_scope();
        assert_eq!(
            config.get_handler(TrapSignal::Return).unwrap().command,
            "echo middle"
        );

        config.exit_suspension_scope();
        assert_eq!(
            config.get_handler(TrapSignal::Return).unwrap().command,
            "echo middle"
        );
    }

    #[test]
    fn child_copy_resolves_inheritance_at_creation_time() {
        let mut config = TrapHandlerConfig::default();
        register(&mut config, TrapSignal::Err, "echo err");
        register(&mut config, TrapSignal::Return, "echo ret");
        register(&mut config, TrapSignal::Exit, "echo exit");

        // Without errtrace/functrace, all of these become display-only in the
        // child: still displayable, but no longer live.
        let options = crate::options::RuntimeOptions::default();
        let copy = config.child_copy(&options);
        assert!(copy.handles(TrapSignal::Err) && !copy.has_live_handler(TrapSignal::Err));
        assert!(copy.handles(TrapSignal::Return) && !copy.has_live_handler(TrapSignal::Return));
        assert!(copy.handles(TrapSignal::Exit) && !copy.has_live_handler(TrapSignal::Exit));
        assert!(config.has_live_handler(TrapSignal::Err));

        // With errtrace, the ERR handler stays live; EXIT is always reset.
        let options = crate::options::RuntimeOptions {
            shell_functions_inherit_err_trap: true,
            ..Default::default()
        };
        let copy = config.child_copy(&options);
        assert!(copy.has_live_handler(TrapSignal::Err));
        assert!(!copy.has_live_handler(TrapSignal::Return));
        assert!(!copy.has_live_handler(TrapSignal::Exit));
    }

    #[test]
    fn trap_modification_discards_display_only_handlers() {
        let mut config = TrapHandlerConfig::default();
        register(&mut config, TrapSignal::Err, "echo err");
        register(&mut config, TrapSignal::Return, "echo ret");

        let options = crate::options::RuntimeOptions::default();
        let mut copy = config.child_copy(&options);
        assert!(copy.handles(TrapSignal::Err));
        assert!(copy.handles(TrapSignal::Return));

        // Registering any trap in the child discards all display-only handlers.
        register(&mut copy, TrapSignal::Debug, "echo debug");
        assert!(!copy.handles(TrapSignal::Err));
        assert!(!copy.handles(TrapSignal::Return));
        assert!(copy.has_live_handler(TrapSignal::Debug));
    }

    #[test]
    fn ignore_dispositions_stay_live_in_children_and_functions() {
        let mut config = TrapHandlerConfig::default();
        register(&mut config, TrapSignal::Err, "");
        register(&mut config, TrapSignal::Exit, "");
        register(&mut config, TrapSignal::Return, "echo ret");

        // Ignore dispositions are inherited by child shells as real state; the
        // non-empty RETURN handler is downgraded to display-only.
        let options = crate::options::RuntimeOptions::default();
        let mut copy = config.child_copy(&options);
        assert!(copy.has_live_handler(TrapSignal::Err));
        assert!(copy.has_live_handler(TrapSignal::Exit));
        assert!(copy.handles(TrapSignal::Return) && !copy.has_live_handler(TrapSignal::Return));

        // A modification in the child discards display-only handlers but not
        // inherited ignore dispositions.
        register(&mut copy, TrapSignal::Debug, "echo debug");
        assert!(copy.handles(TrapSignal::Err));
        assert!(copy.handles(TrapSignal::Exit));
        assert!(!copy.handles(TrapSignal::Return));

        // Ignore dispositions are not suspended at function entry.
        config.enter_suspension_scope([TrapSignal::Err, TrapSignal::Return]);
        assert!(config.handles(TrapSignal::Err));
        assert!(!config.handles(TrapSignal::Return));
        config.exit_suspension_scope();
        assert!(config.handles(TrapSignal::Return));
    }

    #[test]
    fn suspension_scope_leaves_display_only_handlers_visible() {
        let mut config = TrapHandlerConfig::default();
        register(&mut config, TrapSignal::Err, "echo err");

        let options = crate::options::RuntimeOptions::default();
        let mut copy = config.child_copy(&options);

        // Entering a suspension scope (function call) in the child leaves the
        // display-only handler in place for `trap -p`.
        copy.enter_suspension_scope([TrapSignal::Err]);
        assert!(copy.handles(TrapSignal::Err));
        copy.exit_suspension_scope();
        assert!(copy.handles(TrapSignal::Err));
    }
}
