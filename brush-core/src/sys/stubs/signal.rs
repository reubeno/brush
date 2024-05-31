use crate::{error, traps};

pub(crate) fn parse_numeric_signal(_signal: i32) -> Result<traps::TrapSignal, error::Error> {
    Err(error::Error::InvalidSignal)
}

pub(crate) fn parse_os_signal_name(_signal: &str) -> Result<traps::TrapSignal, error::Error> {
    Err(error::Error::InvalidSignal)
}

pub(crate) fn continue_process(_pid: u32) -> Result<(), error::Error> {
    error::unimp("continue process")
}

pub(crate) fn kill_process(_pid: u32) -> Result<(), error::Error> {
    error::unimp("kill process")
}

pub(crate) fn lead_new_process_group() -> Result<(), error::Error> {
    Ok(())
}

pub(crate) struct FakeSignal {}

impl FakeSignal {
    fn new() -> Self {
        Self {}
    }

    pub async fn recv(&self) {
        futures::future::pending::<()>().await;
    }
}

pub(crate) fn tstp_signal_listener() -> Result<FakeSignal, error::Error> {
    Ok(FakeSignal::new())
}

pub(crate) fn chld_signal_listener() -> Result<FakeSignal, error::Error> {
    Ok(FakeSignal::new())
}

pub(crate) async fn await_ctrl_c() -> std::io::Result<()> {
    FakeSignal::new().recv().await;
    Ok(())
}

pub(crate) fn mask_sigttou() -> Result<(), error::Error> {
    Ok(())
}
