pub(crate) use crate::sys::os_pipe as pipes;
pub(crate) use crate::sys::stubs::fs;
pub(crate) use crate::sys::stubs::input;
pub(crate) mod network;
pub(crate) use crate::sys::stubs::resource;

pub(crate) mod signal {
    pub(crate) use crate::sys::stubs::signal::*;
    pub(crate) use tokio::signal::ctrl_c as await_ctrl_c;
}

pub(crate) use crate::sys::stubs::terminal;
pub(crate) use crate::sys::tokio_process as process;
pub(crate) mod users;
