//! What a program that owns its process does to host brush shells.
//!
//! A shell in `brush-core` never changes its process's state: that's for whoever hosts it,
//! which may run several shells in one process, or have uses of its own for the process. This
//! crate is for a host that owns its process, as the `brush` binary does. Each module is one
//! thing such a host does for its shells, behind a feature of its name:
//!
//! * `runtime`: the async runtime the shells run on.
//! * `terminal`: the terminal, and the process group that has it, for an interactive shell's
//!   job control; `ControllingTerminal` is it as a shell's `Terminal`.
//!
//! Nothing here is shell semantics, which are `brush-core`'s, nor presentation, which is
//! `brush-interactive`'s.
//!
//! ```no_run
//! # #[cfg(all(feature = "runtime", feature = "terminal"))]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let runtime = brush_hosting::runtime::builder().build()?;
//! runtime.block_on(async {
//!     // For an interactive shell: ignore SIGTTOU, lead a process group of our own, and take
//!     // the terminal's foreground for it, giving it back when this is dropped. Nothing is
//!     // taken without a terminal on standard input.
//!     let _terminal = brush_hosting::terminal::ControllingTerminal::acquire()?;
//!     // Named as the shell's terminal, it moves the foreground between the shell and its jobs.
//!     type Extensions = brush_core::extensions::ShellExtensionsImpl<
//!         brush_core::extensions::DefaultErrorFormatter,
//!         brush_core::extensions::DefaultProcessLauncher,
//!         brush_hosting::terminal::ControllingTerminal,
//!     >;
//!     let _shell = brush_core::Shell::builder_with_extensions::<Extensions>()
//!         .interactive(true)
//!         .build()
//!         .await?;
//!     // ... run commands in the shell.
//!     Ok::<_, Box<dyn std::error::Error>>(())
//! })?;
//! # Ok(())
//! # }
//! # #[cfg(not(all(feature = "runtime", feature = "terminal")))]
//! # fn main() {}
//! ```

#[cfg(feature = "runtime")]
pub mod runtime;
#[cfg(feature = "terminal")]
pub mod terminal;
