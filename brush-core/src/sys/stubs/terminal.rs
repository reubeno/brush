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

pub(crate) fn is_stdin_a_terminal() -> Result<bool, error::Error> {
    Ok(false)
}

pub(crate) fn get_parent_process_id() -> Option<u32> {
    None
}

pub(crate) fn get_process_group_id() -> Option<u32> {
    None
}

pub(crate) fn get_foreground_pid() -> Option<u32> {
    None
}

pub(crate) fn move_to_foreground(_pid: u32) -> Result<(), error::Error> {
    Ok(())
}

pub(crate) fn move_self_to_foreground() -> Result<(), error::Error> {
    Ok(())
}
