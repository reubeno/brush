/// Holds information about the hosting terminal.
#[derive(Clone, Debug, Default)]
pub struct TerminalInfo {
    /// The detected terminal, if any.
    pub terminal: Option<KnownTerminal>,

    /// If applicable, a session nonce assigned by the terminal.
    pub session_nonce: Option<String>,

    /// Whether the terminal's OSC support is unknown.
    pub osc_support_unknown: bool,

    /// Whether the terminal supports OSC 0 sequences: setting terminal title and icon.
    pub supports_osc_0: bool,

    /// Whether the terminal supports OSC 1 sequences: setting icon name.
    pub supports_osc_1: bool,

    /// Whether the terminal supports OSC 2 sequences: setting terminal title.
    pub supports_osc_2: bool,

    /// Whether the terminal supports OSC 3 sequences: setting X11 window properties.
    pub supports_osc_3: bool,

    /// Whether the terminal supports OSC 4 sequences: setting color palette.
    pub supports_osc_4: bool,

    /// Whether the terminal supports OSC 5 sequences: setting/querying special color number.
    pub supports_osc_5: bool,

    /// Whether the terminal supports OSC 6 sequences: setting title tab color (iTerm2)
    pub supports_osc_6: bool,

    /// Whether the terminal supports OSC 7 sequences: setting current working directory.
    pub supports_osc_7: bool,

    /// Whether the terminal supports OSC 8 sequences: hyperlinks.
    pub supports_osc_8: bool,

    /// Whether the terminal supports OSC 9 sequences: showing system notification (iTerm2).
    pub supports_osc_9: bool,

    /// Whether the terminal supports OSC 10 sequences: setting default foreground color.
    pub supports_osc_10: bool,

    /// Whether the terminal supports OSC 11 sequences: setting default background color.
    pub supports_osc_11: bool,

    /// Whether the terminal supports OSC 12 sequences: setting cursor color.
    pub supports_osc_12: bool,

    /// Whether the terminal supports OSC 13 sequences: setting pointer foreground color.
    pub supports_osc_13: bool,

    /// Whether the terminal supports OSC 14 sequences: setting pointer background color.
    pub supports_osc_14: bool,

    /// Whether the terminal supports OSC 15 sequences: setting Tektronix foreground color.
    pub supports_osc_15: bool,

    /// Whether the terminal supports OSC 16 sequences: setting Tektronix background color.
    pub supports_osc_16: bool,

    /// Whether the terminal supports OSC 17 sequences: setting highlight background color.
    pub supports_osc_17: bool,

    /// Whether the terminal supports OSC 18 sequences: setting Tektronix cursor color.
    pub supports_osc_18: bool,

    /// Whether the terminal supports OSC 19 sequences: setting highlight foreground color.
    pub supports_osc_19: bool,

    /// Whether the terminal supports OSC 21 sequences: color control (Kitty extension).
    pub supports_osc_21: bool,

    /// Whether the terminal supports OSC 22 sequences: setting mouse pointer.
    pub supports_osc_22: bool,

    /// Whether the terminal supports OSC 50 sequences: setting font.
    pub supports_osc_50: bool,

    /// Whether the terminal supports OSC 52 sequences: clipboard and primary selection.
    pub supports_osc_52: bool,

    /// Whether the terminal supports OSC 66 sequences: scoped text size.
    pub supports_osc_66: bool,

    /// Whether the terminal supports OSC 99 sequences: desktop notifications.
    pub supports_osc_99: bool,

    /// Whether the terminal supports OSC 104 sequences: resetting color palette.
    pub supports_osc_104: bool,

    /// Whether the terminal supports OSC 105 sequences: resetting special colors.
    pub supports_osc_105: bool,

    /// Whether the terminal supports OSC 110 sequences: resetting default foreground color.
    pub supports_osc_110: bool,

    /// Whether the terminal supports OSC 111 sequences: resetting default background color.
    pub supports_osc_111: bool,

    /// Whether the terminal supports OSC 112 sequences: resetting cursor color.
    pub supports_osc_112: bool,

    /// Whether the terminal supports OSC 113 sequences: resetting pointer foreground color.
    pub supports_osc_113: bool,

    /// Whether the terminal supports OSC 114 sequences: resetting pointer background color.
    pub supports_osc_114: bool,

    /// Whether the terminal supports OSC 115 sequences: resetting Tektronix foreground color.
    pub supports_osc_115: bool,

    /// Whether the terminal supports OSC 116 sequences: resetting Tektronix background color.
    pub supports_osc_116: bool,

    /// Whether the terminal supports OSC 117 sequences: resetting highlight background color
    pub supports_osc_117: bool,

    /// Whether the terminal supports OSC 118 sequences: resetting Tektronix cursor color.
    pub supports_osc_118: bool,

    /// Whether the terminal supports OSC 119 sequences: resetting highlight foreground color
    pub supports_osc_119: bool,

    /// Whether the terminal supports OSC 133 sequences: shell integration (input, output, and prompt zones).
    pub supports_osc_133: bool,

    /// Whether the terminal supports OSC 176 sequences: setting app ID.
    pub supports_osc_176: bool,

    /// Whether the terminal supports OSC 555 sequences: flashing screen (foot-specific)
    pub supports_osc_555: bool,

    /// Whether the terminal supports OSC 633 sequences: shell integration (`VSCode` extension).
    pub supports_osc_633: bool,

    /// Whether the terminal supports OSC 777 sequences: desktop notifications / rxvt extensions.
    pub supports_osc_777: bool,

    /// Whether the terminal supports OSC 1337 sequences: custom iTerm2 sequences.
    pub supports_osc_1337: bool,

    /// Whether the terminal supports OSC 5113 sequences: file transfer (Kitty extension).
    pub supports_osc_5113: bool,

    /// Whether the terminal supports OSC 5522 sequences: advanced clipboard interaction (Kitty extension).
    pub supports_osc_5522: bool,

    /// Whether the terminal supports OSC 9001 sequences: Windows Terminal extensions.
    pub supports_osc_9001: bool,
}

/// Identifies a known terminal emulator hosting this process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownTerminal {
    /// Alacritty
    Alacritty,
    /// Apple terminal
    AppleTerminal,
    /// Ghostty
    Ghostty,
    /// GNOME Terminal
    GnomeTerminal,
    /// iTerm2
    ITerm2,
    /// Kitty
    Kitty,
    /// Konsole
    Konsole,
    /// `VSCode` Terminal    
    VSCode,
    /// Other VTE-based terminal
    Vte,
    /// Warp Terminal
    WarpTerminal,
    /// `WezTerm`
    WezTerm,
    /// Windows Terminal
    WindowsTerminal,
}

/// Abstracts access to environment variables used for terminal detection.
pub(crate) trait TerminalEnvironment {
    /// Gets the value of an environment variable. Returns `None` if the variable is not set.
    fn get_env_var(&self, key: &str) -> Option<String>;
}

#[allow(clippy::too_many_lines)]
pub(crate) fn get_terminal_info(env: &impl TerminalEnvironment) -> TerminalInfo {
    let mut info = TerminalInfo {
        terminal: try_detect_terminal(env),
        ..Default::default()
    };

    if let Some(terminal) = &info.terminal {
        match terminal {
            KnownTerminal::Alacritty => {
                // https://github.com/alacritty/alacritty/blob/master/docs/escape_support.md
                info.supports_osc_0 = true;
                info.supports_osc_2 = true;
                info.supports_osc_4 = true;
                info.supports_osc_8 = true;
                info.supports_osc_10 = true;
                info.supports_osc_11 = true;
                info.supports_osc_12 = true;
                info.supports_osc_50 = true; // only cursor shape supported
                info.supports_osc_52 = true; // only clipboard and primary selection supported
                info.supports_osc_104 = true;
                info.supports_osc_110 = true;
                info.supports_osc_111 = true;
                info.supports_osc_112 = true;
            }
            KnownTerminal::Ghostty => {
                // https://ghostty.org/docs/vt/osc/0
                info.supports_osc_0 = true;
                info.supports_osc_1 = true;
                info.supports_osc_2 = true;
                info.supports_osc_4 = true;
                info.supports_osc_5 = true;
                info.supports_osc_7 = true;
                info.supports_osc_8 = true;
                info.supports_osc_9 = true;
                info.supports_osc_10 = true;
                info.supports_osc_11 = true;
                info.supports_osc_12 = true;
                info.supports_osc_21 = true;
                info.supports_osc_22 = true;
                info.supports_osc_52 = true;
                info.supports_osc_104 = true;
                info.supports_osc_105 = true;
                info.supports_osc_110 = true;
                info.supports_osc_111 = true;
                info.supports_osc_112 = true;
            }
            KnownTerminal::ITerm2 => {
                // https://iterm2.com/documentation-escape-codes.html
                info.supports_osc_4 = true;
                info.supports_osc_6 = true;
                info.supports_osc_7 = true;
                info.supports_osc_8 = true;
                info.supports_osc_133 = true;
                info.supports_osc_1337 = true;
            }
            KnownTerminal::Kitty => {
                // https://sw.kovidgoyal.net/kitty/protocol-extensions/
                info.supports_osc_21 = true;
                info.supports_osc_22 = true;
                info.supports_osc_66 = true;
                info.supports_osc_5113 = true;
                info.supports_osc_5522 = true;
            }
            KnownTerminal::VSCode => {
                // https://code.visualstudio.com/docs/terminal/shell-integration
                // https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/terminal/browser/terminalEscapeSequences.ts
                info.supports_osc_7 = true;
                info.supports_osc_9 = true;
                info.supports_osc_133 = true;
                info.supports_osc_633 = true;
                info.supports_osc_1337 = true;
                info.session_nonce = env.get_env_var("VSCODE_NONCE");
            }
            KnownTerminal::WezTerm => {
                // https://wezterm.org/escape-sequences.html
                // https://wezterm.org/shell-integration.html
                info.supports_osc_0 = true;
                info.supports_osc_1 = true;
                info.supports_osc_2 = true;
                info.supports_osc_4 = true;
                info.supports_osc_7 = true;
                info.supports_osc_8 = true;
                info.supports_osc_9 = true;
                info.supports_osc_10 = true;
                info.supports_osc_11 = true;
                info.supports_osc_12 = true;
                info.supports_osc_52 = true;
                info.supports_osc_104 = true;
                info.supports_osc_133 = true;
                info.supports_osc_777 = true;
                info.supports_osc_1337 = true;
            }
            KnownTerminal::WindowsTerminal => {
                // https://learn.microsoft.com/en-us/windows/terminal/tutorials/shell-integration
                // https://github.com/microsoft/terminal/blob/main/src/terminal/parser/OutputStateMachineEngine.hpp
                info.supports_osc_0 = true;
                info.supports_osc_1 = true;
                info.supports_osc_2 = true;
                info.supports_osc_4 = true;
                info.supports_osc_8 = true;
                info.supports_osc_9 = true;
                info.supports_osc_10 = true;
                info.supports_osc_11 = true;
                info.supports_osc_12 = true;
                info.supports_osc_17 = true;
                info.supports_osc_21 = true;
                info.supports_osc_52 = true;
                info.supports_osc_104 = true;
                info.supports_osc_110 = true;
                info.supports_osc_111 = true;
                info.supports_osc_112 = true;
                info.supports_osc_117 = true;
                info.supports_osc_133 = true;
                info.supports_osc_633 = true;
                info.supports_osc_1337 = true;
                info.supports_osc_9001 = true;
            }
            _ => {
                info.osc_support_unknown = true;
            }
        }
    }

    info
}

/// Tries to detect the hosting terminal.
///
/// # Arguments
///
/// * `env` - An implementation of `TerminalEnvironment` to access environment variables.
///
pub(crate) fn try_detect_terminal(env: &impl TerminalEnvironment) -> Option<KnownTerminal> {
    if let Some(detected) = try_detect_terminal_from_prog_var(env) {
        Some(detected)
    } else if env.get_env_var("WT_SESSION").is_some() {
        Some(KnownTerminal::WindowsTerminal)
    } else {
        None
    }
}

fn try_detect_terminal_from_prog_var(env: &impl TerminalEnvironment) -> Option<KnownTerminal> {
    let term_prog = env.get_env_var("TERM_PROGRAM")?;

    // Remove punctuation and normalize.
    let term_prog: String = term_prog
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();

    match term_prog.as_str() {
        "alacritty" => Some(KnownTerminal::Alacritty),
        "appleterminal" => Some(KnownTerminal::AppleTerminal),
        "ghostty" => Some(KnownTerminal::Ghostty),
        "gnometerminal" => Some(KnownTerminal::GnomeTerminal),
        "iterm" | "iterm2" | "itermapp" => Some(KnownTerminal::ITerm2),
        "kitty" => Some(KnownTerminal::Kitty),
        "konsole" => Some(KnownTerminal::Konsole),
        "vscode" => Some(KnownTerminal::VSCode),
        "vte" => Some(KnownTerminal::Vte),
        "warp" | "warpterminal" => Some(KnownTerminal::WarpTerminal),
        "wezterm" => Some(KnownTerminal::WezTerm),
        "windowsterminal" => Some(KnownTerminal::WindowsTerminal),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_matches;
    use std::collections::HashMap;

    impl TerminalEnvironment for HashMap<&str, &str> {
        fn get_env_var(&self, key: &str) -> Option<String> {
            self.get(key).map(|v| (*v).to_string())
        }
    }

    #[test]
    fn vscode_recognition() {
        let test_env = HashMap::from([("TERM_PROGRAM", "vscode"), ("VSCODE_NONCE", "test_nonce")]);

        let term_info = get_terminal_info(&test_env);
        assert_matches!(term_info.terminal, Some(KnownTerminal::VSCode));
        assert!(term_info.supports_osc_633);
        assert_eq!(term_info.session_nonce, Some("test_nonce".to_string()));
    }
}
