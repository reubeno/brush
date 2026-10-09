mod input_backend;
mod non_term_line_reader;
mod term_line_reader;

pub use input_backend::BasicInputBackend;

use crate::{ReadResult, ShellError};

pub(crate) trait LineReader {
    fn read_line(
        &self,
        prompt: Option<&str>,
        completion_handler: impl FnMut(&str, usize) -> Result<crate::completion::Offers, ShellError>,
    ) -> Result<ReadResult, ShellError>;
}
