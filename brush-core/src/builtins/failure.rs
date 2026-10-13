//! A builtin's failure in its own words.

use std::error::Error as StdError;
use std::fmt;

use crate::ExecutionExitCode;

/// An ordinary failure, which a builtin describes in its own words: a message,
/// or an error of the builtin's own type.
///
/// For example, `10: shift count out of range`, or `nofunc: not a function`.
/// Return it from `execute` to stop (it converts into a `brush_core::Error`);
/// the shell prints it attributed to the builtin, and the command fails with
/// its [exit code](Self::exit_code). See [Failure](super#failure).
///
/// A failure keeps the error it was made from, as a [`std::io::Error`] keeps a
/// custom error, so a builtin reports its failures in error types of its own,
/// without new error kinds in brush-core. Whatever reports or inspects the
/// failure (an [error formatter](crate::extensions::ErrorFormatter), or a test)
/// gets the error back with [`downcast_ref`](Self::downcast_ref). The failure
/// displays as that error, and its [`source`](StdError::source) is that
/// error's.
///
/// The shell's own errors ([`crate::Error`]) pass through `execute` as they
/// are, rather than becoming failures: a failure is never fatal and has an
/// exit code of its own, so it would hide both of theirs. A failure whose error
/// comes from a broken pipe (the error itself, or one of its causes) isn't
/// printed, as a broken pipe the shell sees isn't.
///
/// # Error types of a builtin's own
///
/// A builtin with an error type of its own converts it into a
/// [`crate::Error`] in one place, choosing each variant's exit code there, so
/// that `?` works in `execute`:
///
/// ```
/// use brush_core::{ExecutionExitCode, builtins::Failure};
///
/// #[derive(Debug, thiserror::Error)]
/// enum CountError {
///     #[error("{0}: count out of range")]
///     OutOfRange(usize),
///     #[error("nothing to count")]
///     Empty,
/// }
///
/// impl From<CountError> for brush_core::Error {
///     fn from(error: CountError) -> Self {
///         let exit_code = match &error {
///             CountError::OutOfRange(_) => ExecutionExitCode::InvalidUsage,
///             CountError::Empty => ExecutionExitCode::GeneralError,
///         };
///         Failure::new(error).with_exit_code(exit_code).into()
///     }
/// }
///
/// let error = brush_core::Error::from(CountError::OutOfRange(11));
/// assert_eq!(error.to_string(), "11: count out of range");
/// assert_eq!(u8::from(ExecutionExitCode::from(&error)), 2);
/// ```
///
/// If the error type wraps a shell error, its conversion returns that error as
/// it is instead.
#[derive(Debug)]
pub struct Failure {
    error: Box<dyn StdError + Send + Sync>,
    exit_code: u8,
}

impl Failure {
    /// Returns a failure described by `error`, with exit code 1.
    ///
    /// # Arguments
    ///
    /// * `error` - What failed, without the builtin's name: a message (a `&str`
    ///   or a `String`), or an error of the builtin's own type, which the
    ///   failure keeps.
    pub fn new(error: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        Self {
            error: error.into(),
            exit_code: 1,
        }
    }

    /// Returns the failure with a different exit code, the status the command
    /// fails with when the builtin returns it. It's 1 unless this says
    /// otherwise.
    ///
    /// # Arguments
    ///
    /// * `exit_code` - The exit code the command fails with.
    #[must_use]
    pub fn with_exit_code(mut self, exit_code: impl Into<ExecutionExitCode>) -> Self {
        self.exit_code = exit_code.into().into();
        self
    }

    /// Returns the exit code the command fails with when the builtin returns
    /// the failure.
    pub fn exit_code(&self) -> ExecutionExitCode {
        self.exit_code.into()
    }

    /// Returns the error the failure was made from, if it's an `E`.
    pub fn downcast_ref<E: StdError + 'static>(&self) -> Option<&E> {
        self.error.downcast_ref()
    }

    /// Returns the I/O error the failure comes from, if any: the error it was
    /// made from, or one of that error's causes. A builtin's error that wraps
    /// a broken pipe is then a broken pipe to the shell, which doesn't print it.
    pub(crate) fn io_error(&self) -> Option<&std::io::Error> {
        let error: &(dyn StdError + 'static) = self.error.as_ref();
        std::iter::successors(Some(error), |&error| error.source())
            .find_map(|error| error.downcast_ref::<std::io::Error>())
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl StdError for Failure {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.error.source()
    }
}
