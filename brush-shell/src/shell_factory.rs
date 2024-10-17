pub(crate) trait ShellFactory {
    type ShellType: brush_interactive::InteractiveShell + Send;

    async fn create(
        &self,
        options: &brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError>;
}

#[allow(dead_code)]
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
        _prompt: brush_interactive::InteractivePrompt,
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

pub(crate) struct ReedlineShellFactory;

impl ShellFactory for ReedlineShellFactory {
    #[cfg(all(feature = "reedline", any(windows, unix)))]
    type ShellType = brush_interactive::ReedlineShell;
    #[cfg(any(not(feature = "reedline"), not(any(windows, unix))))]
    type ShellType = StubShell;

    #[allow(unused)]
    async fn create(
        &self,
        options: &brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError> {
        #[cfg(any(windows, unix))]
        {
            brush_interactive::ReedlineShell::new(options).await
        }
        #[cfg(not(any(windows, unix)))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}

pub(crate) struct BasicShellFactory;

impl ShellFactory for BasicShellFactory {
    #[cfg(feature = "basic")]
    type ShellType = brush_interactive::BasicShell;
    #[cfg(not(feature = "basic"))]
    type ShellType = StubShell;

    #[allow(unused)]
    async fn create(
        &self,
        options: &brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError> {
        #[cfg(feature = "basic")]
        {
            brush_interactive::BasicShell::new(options).await
        }
        #[cfg(not(feature = "basic"))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}
