pub(crate) trait InputBackendFactory {
    type InputBackendType: brush_interactive::InputBackend + Send;

    fn create(
        &self,
        options: brush_interactive::UIOptions,
        shell_ref: &brush_interactive::ShellRef,
    ) -> Result<Self::InputBackendType, brush_interactive::ShellError>;
}

pub(crate) struct ReedlineInputBackendFactory;

#[allow(unused_variables, reason = "options are not used on all platforms")]
impl InputBackendFactory for ReedlineInputBackendFactory {
    #[cfg(all(feature = "reedline", any(unix, windows)))]
    type InputBackendType = brush_interactive::ReedlineInputBackend;
    #[cfg(any(not(feature = "reedline"), not(any(unix, windows))))]
    type InputBackendType = StubShell;

    fn create(
        &self,
        options: brush_interactive::UIOptions,
        shell_ref: &brush_interactive::ShellRef,
    ) -> Result<Self::InputBackendType, brush_interactive::ShellError> {
        #[cfg(all(feature = "reedline", any(unix, windows)))]
        {
            brush_interactive::ReedlineInputBackend::new(&options, shell_ref)
        }
        #[cfg(any(not(feature = "reedline"), not(any(unix, windows))))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}

pub(crate) struct BasicInputBackendFactory;

#[allow(unused_variables, reason = "options are not used on all platforms")]
impl InputBackendFactory for BasicInputBackendFactory {
    #[cfg(feature = "basic")]
    type InputBackendType = brush_interactive::BasicInputBackend;
    #[cfg(not(feature = "basic"))]
    type InputBackendType = StubShell;

    fn create(
        &self,
        _options: brush_interactive::UIOptions,
        _shell_ref: &brush_interactive::ShellRef,
    ) -> Result<Self::InputBackendType, brush_interactive::ShellError> {
        #[cfg(feature = "basic")]
        {
            Ok(brush_interactive::BasicInputBackend)
        }
        #[cfg(not(feature = "basic"))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}

pub(crate) struct MinimalInputBackendFactory;

impl InputBackendFactory for MinimalInputBackendFactory {
    #[cfg(feature = "minimal")]
    type InputBackendType = brush_interactive::MinimalInputBackend;
    #[cfg(not(feature = "minimal"))]
    type InputBackendType = StubShell;

    #[allow(unused_variables, reason = "options are not used on all platforms")]
    fn create(
        &self,
        _options: brush_interactive::UIOptions,
        _shell_ref: &brush_interactive::ShellRef,
    ) -> Result<Self::InputBackendType, brush_interactive::ShellError> {
        #[cfg(feature = "minimal")]
        {
            Ok(brush_interactive::MinimalInputBackend)
        }
        #[cfg(not(feature = "minimal"))]
        {
            Err(brush_interactive::ShellError::InputBackendNotSupported)
        }
    }
}
