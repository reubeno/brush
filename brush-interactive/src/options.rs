/// Options for creating an interactive shell.
pub struct Options {
    /// Lower-level options for creating the shell.
    pub shell: brush_core::CreateOptions,
    /// Whether to disable bracketed paste mode.
    pub disable_bracketed_paste: bool,
}
