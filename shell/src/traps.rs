use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct TrapHandlerConfig {
    pub handlers: HashMap<nix::sys::signal::Signal, Vec<String>>,
}

impl TrapHandlerConfig {
    pub fn register_handler(&mut self, signal_type: nix::sys::signal::Signal, command: String) {
        if let Some(handlers) = self.handlers.get_mut(&signal_type) {
            handlers.push(command);
        } else {
            self.handlers.insert(signal_type, vec![command]);
        }
    }

    pub fn remove_handlers(&mut self, signal_type: nix::sys::signal::Signal) {
        self.handlers.remove(&signal_type);
    }
}
