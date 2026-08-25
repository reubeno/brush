//! Engine-neutral contract for parsing builtin command arguments.
//!
//! A builtin's argument handling is expressed as a plain type that
//! implements [`FromArgs`]: given the words following the builtin name,
//! produce either an instance of the type or an [`ArgsError`]. Builtins
//! themselves never depend on a particular argument-parsing engine; the
//! engine is selected at compile time and provides the implementations.
//!
//! ```no_run
//! use brush_core::args::FromArgs;
//!
//! struct ShiftArgs {
//!     n: Option<i32>,
//! }
//!
//! impl FromArgs for ShiftArgs {
//!     fn from_args(words: &[String]) -> Result<Self, brush_core::args::ArgsError> {
//!         // Engine-provided helper or hand-rolled parsing goes here.
//! #       Ok(Self { n: None })
//!     }
//! }
//! ```

/// Error produced while parsing a builtin's arguments.
///
/// Distinguishes ordinary usage failures (which surface as invalid-usage
/// exit codes) from help/version requests (which surface by printing to
/// stdout and exiting successfully).
#[derive(Clone, Debug)]
pub struct ArgsError {
    /// Human-readable description of the failure, already rendered in the
    /// style the shell prints for the originating builtin.
    pub message: String,

    /// Whether this "error" is actually a request to display help (or
    /// version) information.
    pub help_request: bool,
}

impl std::fmt::Display for ArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl ArgsError {
    /// Constructs a usage error with the given message.
    ///
    /// # Arguments
    ///
    /// * `message` - The error message to present to the user.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            help_request: false,
        }
    }

    /// Constructs an error representing a request to display help text.
    ///
    /// # Arguments
    ///
    /// * `message` - The rendered help text to present to the user.
    pub fn help(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            help_request: true,
        }
    }

    /// Converts an engine-native parse error into an [`ArgsError`].
    ///
    /// # Arguments
    ///
    /// * `err` - The error to convert.
    // N.B. Transitional: clap remains an unconditional dependency of
    // brush-core until the migration completes.
    pub fn from_clap_error(err: &clap::Error) -> Self {
        let help_request = matches!(
            err.kind(),
            clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
        );
        Self {
            message: err.to_string(),
            help_request,
        }
    }
}

/// Contract for turning the words following a builtin's name into a typed
/// value.
///
/// Implementations are provided per argument-parsing engine; builtins consume
/// only this trait and remain agnostic of the engine beneath it.
pub trait FromArgs: Sized {
    /// Parses the given words into an instance of the implementing type.
    ///
    /// # Arguments
    ///
    /// * `words` - The arguments as they appeared after expansion, excluding
    ///   the builtin's own name.
    fn from_args(words: &[String]) -> Result<Self, ArgsError>;
}

// N.B. Transitional blanket implementation: while builtins migrate off of
// direct engine coupling, any clap-derived type satisfies the contract
// automatically. It is removed once no builtin relies on it.
//
// N.B. Until dispatch stops passing the invocation verbatim, `words` includes
// the builtin's own name at index 0, occupying clap's bin-name slot.
impl<T: clap::Parser> FromArgs for T {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        <T as clap::Parser>::try_parse_from(words.iter().map(String::as_str))
            .map_err(|err| ArgsError::from_clap_error(&err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    #[clap(disable_help_flag = true, disable_version_flag = true)]
    struct TestArgs {
        #[arg(short = 'n')]
        flag: bool,

        #[arg(short = 'd')]
        value: Option<String>,

        operands: Vec<String>,
    }

    #[test]
    #[allow(clippy::panic)]
    fn blanket_impl_parses_flags_and_values() {
        // N.B. Transitional contract: invocation includes the builtin name.
        let parsed = TestArgs::from_args(&[
            "echo".to_string(),
            "-n".to_string(),
            "-d".to_string(),
            ":".to_string(),
            "a".to_string(),
        ])
        .unwrap_or_else(|e| panic!("parsing should succeed: {e}"));

        assert!(parsed.flag);
        assert_eq!(parsed.value.as_deref(), Some(":"));
        assert_eq!(parsed.operands, ["a"]);
    }

    #[test]
    #[allow(clippy::panic)]
    fn blanket_impl_reports_usage_errors() {
        let Err(err) = TestArgs::from_args(&["echo".to_string(), "--frobnicate".to_string()])
        else {
            panic!("unknown flag should fail");
        };

        assert!(!err.help_request);
        assert!(!err.message.is_empty());
    }

    #[test]
    fn args_error_constructors_set_help_request() {
        assert!(!ArgsError::new("boom").help_request);
        assert!(ArgsError::help("help text").help_request);
    }
}
