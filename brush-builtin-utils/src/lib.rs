//! Utilities for implementing builtins for brush.
//!
//! `brush-core` defines the contracts a builtin implements and depends on no
//! argument-parsing engine. This crate supplies the conveniences that make
//! those contracts easy to satisfy with a particular engine. Each engine sits
//! behind its own feature; [`verbatim`], which parses nothing, needs none.

mod args;
#[cfg(feature = "clap")]
pub mod clap_adapter;
pub mod verbatim;

// Lets the exported macros name brush-core items from any calling crate.
#[doc(hidden)]
pub use brush_core as __brush_core;
