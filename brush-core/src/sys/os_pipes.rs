//! Anonymous pipes backed by the operating system.

pub(crate) use std::io::{PipeReader, PipeWriter, pipe};
