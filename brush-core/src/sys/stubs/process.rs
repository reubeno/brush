pub(crate) type ProcessId = i32;

pub(crate) struct Child {
    inner: std::process::Child,
}

pub(crate) use std::process::ExitStatus;
pub(crate) use std::process::Output;

impl Child {
    pub fn id(&self) -> Option<u32> {
        None
    }

    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.inner.wait()
    }

    pub async fn wait_with_output(self) -> std::io::Result<Output> {
        self.inner.wait_with_output()
    }
}

pub(crate) fn spawn(mut command: std::process::Command) -> std::io::Result<Child> {
    let child = command.spawn()?;
    Ok(Child { inner: child })
}
