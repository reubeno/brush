//! Per-engine argument parsing for builtins.
//!
//! Each engine module owns two things for every converted builtin:
//!
//! 1. an [`brush_core::args::FromArgs`] implementation binding words to the
//!    builtin's plain argument struct, and
//! 2. help rendering, until brush grows its own engine-neutral help model.

pub mod clap;
