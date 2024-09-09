#[async_trait::async_trait]
pub(crate) trait ShellFactory {
    type ShellType: brush_interactive::InteractiveShell + Send;

    async fn create(
        &self,
        options: &brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError>;
}

pub(crate) struct StubShell;

#[allow(clippy::panic)]
impl brush_interactive::InteractiveShell for StubShell {
    #[allow(unreachable_code)]
    fn shell(&self) -> impl AsRef<brush_core::Shell> + Send {
        panic!("No interactive shell implementation available");
        self
    }

    #[allow(unreachable_code)]
    fn shell_mut(&mut self) -> impl AsMut<brush_core::Shell> + Send {
        panic!("No interactive shell implementation available");
        self
    }

    fn read_line(
        &mut self,
        _prompt: &str,
    ) -> Result<brush_interactive::ReadResult, brush_interactive::ShellError> {
        Err(brush_interactive::ShellError::InputBackendNotSupported)
    }

    fn update_history(&mut self) -> Result<(), brush_interactive::ShellError> {
        Err(brush_interactive::ShellError::InputBackendNotSupported)
    }
}

#[allow(clippy::panic)]
impl AsRef<brush_core::Shell> for StubShell {
    fn as_ref(&self) -> &brush_core::Shell {
        panic!("No interactive shell implementation available")
    }
}

#[allow(clippy::panic)]
impl AsMut<brush_core::Shell> for StubShell {
    fn as_mut(&mut self) -> &mut brush_core::Shell {
        panic!("No interactive shell implementation available")
    }
}

pub(crate) struct RustylineShellFactory;

#[async_trait::async_trait]
impl ShellFactory for RustylineShellFactory {
    #[cfg(any(windows, unix))]
    type ShellType = brush_interactive::RustylineShell;
    #[cfg(not(any(windows, unix)))]
    type ShellType = StubShell;

    #[allow(unused)]
    async fn create(
        &self,
        options: &brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError> {
        #[cfg(any(windows, unix))]
        {
            brush_interactive::RustylineShell::new(options).await
        }
        #[cfg(not(any(windows, unix)))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}

pub(crate) struct BasicShellFactory;

#[async_trait::async_trait]
impl ShellFactory for BasicShellFactory {
    #[cfg(not(any(windows, unix)))]
    type ShellType = brush_interactive::BasicShell;
    #[cfg(any(windows, unix))]
    type ShellType = StubShell;

    #[allow(unused)]
    async fn create(
        &self,
        options: &brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError> {
        #[cfg(not(any(windows, unix)))]
        {
            brush_interactive::BasicShell::new(options).await
        }
        #[cfg(any(windows, unix))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}
