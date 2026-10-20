//! What a program that owns its process does to host brush shells.
//!
//! A shell in `brush-core` never changes its process's state: that's for whoever hosts it,
//! which may run several shells in one process, or have uses of its own for the process. This
//! crate is for a host that owns its process, as the `brush` binary does. Each module is one
//! thing such a host does for its shells, behind a feature of its name:
//!
//! * `runtime`: the async runtime the shells run on.
//! * `terminal`: the terminal, and the process group that has it, for an interactive shell's
//!   job control (`ProcessTerminal`).

#[cfg(feature = "runtime")]
pub mod runtime;
#[cfg(feature = "terminal")]
pub mod terminal;
