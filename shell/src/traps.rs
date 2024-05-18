use std::{collections::HashMap, fmt::Display};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum TrapSignal {
    Signal(nix::sys::signal::Signal),
    Debug,
    Err,
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

#[derive(Clone, Default)]
pub struct TrapHandlerConfig {
    pub handlers: HashMap<TrapSignal, String>,
    pub handler_depth: i32,
}

impl TrapHandlerConfig {
    pub fn register_handler(&mut self, signal_type: TrapSignal, command: String) {
        let _ = self.handlers.insert(signal_type, command);
    }

    pub fn remove_handlers(&mut self, signal_type: TrapSignal) {
        self.handlers.remove(&signal_type);
    }
}
