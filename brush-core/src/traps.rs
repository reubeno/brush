use std::str::FromStr;
use std::{collections::HashMap, fmt::Display};

use itertools::Itertools as _;

use crate::error;

/// Type of signal that can be trapped in the shell.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum TrapSignal {
    /// A system signal.
    #[cfg(unix)]
    Signal(nix::sys::signal::Signal),
    /// The `DEBUG` trap.
    Debug,
    /// The `ERR` trap.
    Err,
    /// The `EXIT` trap.
    Exit,
}

impl Display for TrapSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TrapSignal {
    /// Returns all possible values of [`TrapSignal`].
    pub fn iterator() -> impl Iterator<Item = TrapSignal> {
        const SIGNALS: &[TrapSignal] = &[TrapSignal::Debug, TrapSignal::Err, TrapSignal::Exit];
        let iter = SIGNALS.iter().copied();

        #[cfg(unix)]
        let iter = itertools::chain!(
            iter,
            nix::sys::signal::Signal::iterator().map(TrapSignal::Signal)
        );

        iter
    }

    /// Converts [`TrapSignal`] into its corresponding signal name as a [`&'static str`](str)
    pub const fn as_str(self) -> &'static str {
        match self {
            #[cfg(unix)]
            TrapSignal::Signal(s) => s.as_str(),
            TrapSignal::Debug => "DEBUG",
            TrapSignal::Err => "ERR",
            TrapSignal::Exit => "EXIT",
        }
    }
}

/// Formats [`Iterator<Item = TrapSignal>`](TrapSignal)  to the provided writer.
///
/// # Arguments
///
/// * `f` - Any type that implements [`std::io::Write`].
/// * `it` - An iterator over the signals that will be formatted into the `f`.
pub fn format_signals(
    mut f: impl std::io::Write,
    it: impl Iterator<Item = TrapSignal>,
) -> Result<(), error::Error> {
    let it = it
        .sorted_by(|a, b| Ord::cmp(&i32::from(*a), &i32::from(*b)))
        .format_with("\n", |s, f| f(&format_args!("{}) {s}", i32::from(s))));
    write!(f, "{it}")?;
    Ok(())
}

// implement s.parse::<TrapSignal>()
impl FromStr for TrapSignal {
    type Err = error::Error;
    fn from_str(s: &str) -> Result<Self, <TrapSignal as FromStr>::Err> {
        if let Ok(n) = s.parse::<i32>() {
            TrapSignal::try_from(n)
        } else {
            TrapSignal::try_from(s)
        }
    }
}

// from a signal number
impl TryFrom<i32> for TrapSignal {
    type Error = error::Error;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            32 => TrapSignal::Debug,
            33 => TrapSignal::Err,
            0 => TrapSignal::Exit,
            #[cfg(unix)]
            value => TrapSignal::Signal(
                nix::sys::signal::Signal::try_from(value)
                    .map_err(|_| error::Error::InvalidSignal(value.to_string()))?,
            ),
            #[cfg(not(unix))]
            _ => return Err(error::Error::InvalidSignal(value.to_string())),
        })
    }
}

// from a signal name
impl TryFrom<&str> for TrapSignal {
    type Error = error::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut s = value.to_ascii_uppercase();
        // Bash compatibility:
        // support for signal names without the `SIG` prefix, for example `HUP` -> `SIGHUP`
        if !s.starts_with("SIG") {
            s.insert_str(0, "SIG");
        }

        Ok(match s.as_str() {
            "SIGDEBUG" => TrapSignal::Debug,
            "SIGERR" => TrapSignal::Err,
            "SIGEXIT" => TrapSignal::Exit,

            #[cfg(unix)]
            _ => nix::sys::signal::Signal::from_str(s.as_str())
                .map(TrapSignal::Signal)
                .map_err(|_| error::Error::InvalidSignal(value.into()))?,
            #[cfg(not(unix))]
            _ => return Err(error::Error::InvalidSignal(value.into())),
        })
    }
}

impl From<TrapSignal> for i32 {
    fn from(value: TrapSignal) -> Self {
        match value {
            #[cfg(unix)]
            TrapSignal::Signal(s) => s as i32,
            TrapSignal::Debug => 32,
            TrapSignal::Err => 33,
            TrapSignal::Exit => 0,
        }
    }
}

/// Configuration for trap handlers in the shell.
#[derive(Clone, Default)]
pub struct TrapHandlerConfig {
    /// Registered handlers for traps; maps signal type to command.
    pub handlers: HashMap<TrapSignal, String>,
    /// Current depth of the handler stack.
    pub handler_depth: i32,
}

impl TrapHandlerConfig {
    /// Registers a handler for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to register a handler for.
    /// * `command` - The command to execute when the signal is trapped.
    pub fn register_handler(&mut self, signal_type: TrapSignal, command: String) {
        let _ = self.handlers.insert(signal_type, command);
    }

    /// Removes handlers for a trap signal.
    ///
    /// # Arguments
    ///
    /// * `signal_type` - The type of signal to remove handlers for.
    pub fn remove_handlers(&mut self, signal_type: TrapSignal) {
        self.handlers.remove(&signal_type);
    }
}
