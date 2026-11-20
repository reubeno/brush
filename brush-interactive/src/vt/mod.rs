//! A line-editing input backend that needs nothing from the terminal but bytes.
//!
//! Keys are decoded from standard input, and the line is drawn on standard error,
//! with VT100-style escape sequences or with backspaces alone.

mod completion_list;
mod input_backend;
mod keymap;
mod keys;
mod line_buffer;
mod line_editor;
mod line_reader;
mod render;
mod style;
mod terminal;

pub use input_backend::VtInputBackend;
pub use terminal::{StdTerminal, Terminal};
