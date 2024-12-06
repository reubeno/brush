use crate::{InteractivePrompt, ReadResult, ShellError};

pub(crate) fn read_input_line_from_terminal(
    _prompt: &InteractivePrompt,
    _completion_handler: impl FnMut(
        &str,
        usize,
    ) -> Result<brush_core::completion::Completions, ShellError>,
) -> Result<ReadResult, ShellError> {
    let mut read_buffer = String::new();

    let bytes_read = std::io::stdin()
        .read_line(&mut read_buffer)
        .map_err(|_err| ShellError::InputError)?;

    if bytes_read != 0 {
        Ok(ReadResult::Input(read_buffer))
    } else {
        Ok(ReadResult::Eof)
    }
}
