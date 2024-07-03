use crate::error;

#[derive(Clone)]
pub(crate) struct TerminalSettings {}

impl TerminalSettings {
    pub fn set_canonical(&mut self, _value: bool) {
        // TODO: implement
    }

    pub fn set_echo(&mut self, _value: bool) {
        // TODO: implement
    }

    pub fn set_int_signal(&mut self, _value: bool) {
        // TODO: implement
    }
}

pub(crate) fn get_term_attr<Fd>(_fd: Fd) -> Result<TerminalSettings, error::Error> {
    Ok(TerminalSettings {})
}

pub(crate) fn set_term_attr_now<Fd>(
    _fd: Fd,
    _settings: &TerminalSettings,
) -> Result<(), error::Error> {
    Ok(())
}
