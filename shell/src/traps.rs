use std::{collections::HashMap, fmt::Display};

/// Type of signal that can be trapped in the shell.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum TrapSignal {
    /// A system signal.
    Signal(nix::sys::signal::Signal),
    /// The `DEBUG` trap.
    Debug,
    /// The `ERR` trap.
    Err,
    /// The `EXIT` trap.
    Exit,
}

impl Display for TrapSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrapSignal::Signal(s) => s.fmt(f),
            TrapSignal::Debug => write!(f, "DEBUG"),
            TrapSignal::Err => write!(f, "ERR"),
            TrapSignal::Exit => write!(f, "EXIT"),
        }
    }
}

impl TrapSignal {
    /// Returns all possible values of `TrapSignal`.
    pub fn all_values() -> Vec<TrapSignal> {
        let mut signals = vec![TrapSignal::Debug, TrapSignal::Err, TrapSignal::Exit];

        for signal in nix::sys::signal::Signal::iterator() {
            signals.push(TrapSignal::Signal(signal));
        }

        signals
    }
}

/// Configuration for trap handlers in the shell.
#[derive(Clone, Default)]
pub struct TrapHandlerConfig {
    /// Registered handlers for traps; maps signal type to command.
    pub handlers: HashMap<TrapSignal, String>,
    /// Current depth of the handler stack.
    pub handler_depth: i32,
}

impl TrapHandlerConfig {
    /// Registers a handler for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to register a handler for.
    /// * `command` - The command to execute when the signal is trapped.
    pub fn register_handler(&mut self, signal_type: TrapSignal, command: String) {
        let _ = self.handlers.insert(signal_type, command);
    }

    /// Removes handlers for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to remove handlers for.
    pub fn remove_handlers(&mut self, signal_type: TrapSignal) {
        self.handlers.remove(&signal_type);
    }
}
