/// Options for a shell user interface.
#[derive(Default, bon::Builder)]
pub struct UIOptions {
    /// Whether to disable bracketed paste mode.
    #[builder(default)]
    pub disable_bracketed_paste: bool,
    /// Whether to disable color.
    #[builder(default)]
    pub disable_color: bool,
    /// Whether to disable syntax highlighting.
    #[builder(default)]
    pub disable_highlighting: bool,
    /// Whether to enable terminal integration.
    #[builder(default)]
    pub terminal_shell_integration: bool,
}
