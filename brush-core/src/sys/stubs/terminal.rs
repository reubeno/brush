use crate::{error, sys};

#[derive(Clone)]
pub(crate) struct TerminalSettings {}

impl TerminalSettings {
    pub fn set_canonical(&mut self, _value: bool) {}
    pub fn set_echo(&mut self, _value: bool) {}
    pub fn set_int_signal(&mut self, _value: bool) {}
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

pub(crate) fn get_parent_process_id() -> Option<sys::process::ProcessId> {
    None
}

pub(crate) fn get_process_group_id() -> Option<sys::process::ProcessId> {
    None
}

pub(crate) fn get_foreground_pid() -> Option<sys::process::ProcessId> {
    None
}

pub(crate) fn move_to_foreground(_pid: sys::process::ProcessId) -> Result<(), error::Error> {
    Ok(())
}

pub(crate) fn move_self_to_foreground() -> Result<(), error::Error> {
    Ok(())
}
