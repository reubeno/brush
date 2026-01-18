use crate::ShellError;

/// Represents an input backend for reading lines of input.
pub trait InputBackend: Send {
    /// Reads a line of input, using the given prompt.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance for which input is being read.
    /// * `prompt` - The prompt to display to the user.
    fn read_line(
        &mut self,
        shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError>;

    /// Returns the current contents of the read buffer and the current cursor
    /// position within the buffer; None is returned if the read buffer is
    /// empty or cannot be read by this implementation.
    fn get_read_buffer(&self) -> Option<(String, usize)> {
        None
    }

    /// Updates the read buffer with the given string and cursor. Considered a
    /// no-op if the implementation does not support updating read buffers.
    fn set_read_buffer(&mut self, _buffer: String, _cursor: usize) {
        // No-op by default.
    }
}

/// Result of a read operation.
pub enum ReadResult {
    /// The user entered a line of input.
    Input(String),
    /// A bound key sequence yielded a registered command.
    BoundCommand(String),
    /// End of input was reached.
    Eof,
    /// The user interrupted the input operation.
    Interrupted,
}

/// Represents an interactive prompt.
pub struct InteractivePrompt {
    /// Prompt to display.
    pub prompt: String,
    /// Alternate-side prompt (typically right) to display.
    pub alt_side_prompt: String,
    /// Prompt to display on a continuation line of input.
    pub continuation_prompt: String,
}
