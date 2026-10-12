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
    /// Whether to enable zsh-style hooks.
    #[builder(default)]
    pub zsh_style_hooks: bool,
    /// Whether, like readline's `show-all-if-ambiguous`, to list several completion
    /// candidates as soon as they're completed to their common prefix, rather than on the
    /// next completion. For now, only the basic input backend honors it.
    #[builder(default)]
    pub show_all_if_ambiguous: bool,
}

impl From<&UIOptions> for crate::InteractiveOptions {
    fn from(options: &UIOptions) -> Self {
        Self {
            terminal_shell_integration: options.terminal_shell_integration,
            zsh_style_hooks: options.zsh_style_hooks,
        }
    }
}
