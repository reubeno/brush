pub(crate) trait ShellFactory {
    type ShellType: brush_interactive::InteractiveShell + Send;

    async fn create(
        &self,
        options: brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError>;
}

#[allow(dead_code, reason = "unused on some platforms")]
pub(crate) struct StubShell;

#[expect(clippy::panic)]
impl brush_interactive::InteractiveShell for StubShell {
    #[expect(unreachable_code)]
    fn shell(&self) -> impl AsRef<brush_core::Shell> + Send {
        panic!("No interactive shell implementation available");
        self
    }

    #[expect(unreachable_code)]
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
}

#[expect(clippy::panic)]
impl AsRef<brush_core::Shell> for StubShell {
    fn as_ref(&self) -> &brush_core::Shell {
        panic!("No interactive shell implementation available")
    }
}

#[expect(clippy::panic)]
impl AsMut<brush_core::Shell> for StubShell {
    fn as_mut(&mut self) -> &mut brush_core::Shell {
        panic!("No interactive shell implementation available")
    }
}

pub(crate) struct ReedlineShellFactory;

#[allow(unused_variables, reason = "options are not used on all platforms")]
impl ShellFactory for ReedlineShellFactory {
    #[cfg(all(feature = "reedline", any(unix, windows)))]
    type ShellType = brush_interactive::ReedlineShell;
    #[cfg(any(not(feature = "reedline"), not(any(unix, windows))))]
    type ShellType = StubShell;

    async fn create(
        &self,
        options: brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError> {
        #[cfg(any(unix, windows))]
        {
            brush_interactive::ReedlineShell::new(options).await
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}

pub(crate) struct BasicShellFactory;

#[allow(unused_variables, reason = "options are not used on all platforms")]
impl ShellFactory for BasicShellFactory {
    #[cfg(feature = "basic")]
    type ShellType = brush_interactive::BasicShell;
    #[cfg(not(feature = "basic"))]
    type ShellType = StubShell;

    async fn create(
        &self,
        options: brush_interactive::Options,
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

pub(crate) struct MinimalShellFactory;

impl ShellFactory for MinimalShellFactory {
    #[cfg(feature = "minimal")]
    type ShellType = brush_interactive::MinimalShell;
    #[cfg(not(feature = "minimal"))]
    type ShellType = StubShell;

    #[allow(unused_variables, reason = "options are not used on all platforms")]
    async fn create(
        &self,
        options: brush_interactive::Options,
    ) -> Result<Self::ShellType, brush_interactive::ShellError> {
        #[cfg(feature = "minimal")]
        {
            brush_interactive::MinimalShell::new(options).await
        }
        #[cfg(not(feature = "minimal"))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}
