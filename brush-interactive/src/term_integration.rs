use std::borrow::Cow;
use std::fmt::Write;

use crate::term_detection;

/// Utility for integrating with terminal emulators.
#[derive(Default)]
pub(crate) struct TerminalIntegration {
    /// Info about the hosting terminal.
    term: term_detection::TerminalInfo,
}

#[allow(dead_code)]
impl TerminalIntegration {
    /// Creates a new terminal integration utility.
    ///
    /// # Arguments
    ///
    /// * `term_info` - Information about the terminal capabilities.
    pub const fn new(term_info: term_detection::TerminalInfo) -> Self {
        Self { term: term_info }
    }

    /// Returns the terminal escape sequence that should be emitted to initialize terminal integration.
    pub fn initialize(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;P;HasRichCommandDetection=True\x1b\\".into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted before the prompt.
    pub fn pre_prompt(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;A\x1b\\".into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence to report the current working directory.
    pub fn report_cwd(&self, cwd: &std::path::Path) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            format!("\x1b]633;P;Cwd={}\x1b\\", cwd.to_string_lossy()).into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted before executing a command,
    /// but after the prompt and the user has finished entering input.
    ///
    /// # Arguments
    ///
    /// * `command` - The command that is about to be executed.
    pub fn pre_exec_command(&self, command: &str) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            let mut escaped_command = escape_command_for_osc_633(command);
            escaped_command.insert_str(0, "\x1b]633;E;");

            if let Some(session_nonce) = &self.term.session_nonce {
                escaped_command.push(';');
                escaped_command.push_str(session_nonce);
            }

            escaped_command.push_str("\x1b\\\x1b]633;C\x1b\\");

            escaped_command.into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted after executing a command.
    pub fn post_exec_command(&self, exit_code: i32) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            std::format!("\x1b]633;D;{exit_code}\x1b\\").into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted after the prompt.
    pub fn post_prompt(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;B\x1b\\".into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted before the continuation prompt.
    pub fn pre_input_line_continuation(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;F\x1b\\".into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted after the input line continuation.
    pub fn post_input_line_continuation(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;G\x1b\\".into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted before the right-side prompt.
    pub fn pre_right_prompt(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;H\x1b\\".into()
        } else {
            "".into()
        }
    }

    /// Returns the terminal escape sequence that should be emitted after the right-side prompt.
    pub fn post_right_prompt(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;I\x1b\\".into()
        } else {
            "".into()
        }
    }
}

fn escape_command_for_osc_633(command: &str) -> String {
    let mut result = String::new();

    for c in command.chars() {
        match c {
            // Escape ASCII control characters (< 0x1f, i.e., < 31)
            '\x00'..='\x1e' => {
                let _ = write!(result, r"\x{:02x}", c as u8);
            }
            // Escape backslash with an extra prefixed backslash
            '\\' => result.push_str(r"\\"),
            // Escape semicolon via \xNN syntax (like control chars)
            ';' => result.push_str(r"\x3b"),
            // Keep other characters as-is
            _ => result.push(c),
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc_633_escaping_basic() {
        // Test simple alphanumeric string
        assert_eq!(escape_command_for_osc_633("echo hello"), "echo hello");
        assert_eq!(escape_command_for_osc_633("ls -la"), "ls -la");
    }

    #[test]
    fn osc_633_escaping_semicolon() {
        // Semicolons should be escaped
        assert_eq!(escape_command_for_osc_633("cmd1; cmd2"), r"cmd1\x3b cmd2");
        assert_eq!(escape_command_for_osc_633(";"), r"\x3b");
        assert_eq!(escape_command_for_osc_633("a;b;c"), r"a\x3bb\x3bc");
    }

    #[test]
    fn osc_633_escaping_backslash() {
        // Backslashes should be escaped
        assert_eq!(escape_command_for_osc_633(r"echo \n"), r"echo \\n");
        assert_eq!(escape_command_for_osc_633(r"\"), r"\\");
        assert_eq!(
            escape_command_for_osc_633(r"C:\path\to\file"),
            r"C:\\path\\to\\file"
        );
    }

    #[test]
    fn osc_633_escaping_control_chars() {
        // ASCII control characters (0x00-0x1e, i.e., 0-30) should be escaped
        assert_eq!(escape_command_for_osc_633("\x00"), r"\x00");
        assert_eq!(escape_command_for_osc_633("\x01"), r"\x01");
        assert_eq!(escape_command_for_osc_633("\t"), r"\x09"); // tab
        assert_eq!(escape_command_for_osc_633("\n"), r"\x0a"); // newline
        assert_eq!(escape_command_for_osc_633("\r"), r"\x0d"); // carriage return
        assert_eq!(escape_command_for_osc_633("\x1e"), r"\x1e"); // last control char (30)

        // 0x1e (30) *should* be escaped as a control char
        assert_eq!(escape_command_for_osc_633("\x1e"), r"\x1e");

        // 0x1f (31) should NOT be escaped as a control char (not < 31)
        assert_eq!(escape_command_for_osc_633("\x1f"), "\x1f");

        // Space (0x20, 32) should NOT be escaped
        assert_eq!(escape_command_for_osc_633(" "), " ");
    }

    #[test]
    fn osc_633_escaping_mixed() {
        // Test combinations of different escape scenarios
        assert_eq!(
            escape_command_for_osc_633("echo\nhello; world\\n"),
            r"echo\x0ahello\x3b world\\n"
        );

        assert_eq!(
            escape_command_for_osc_633("cmd\t\t; \\path"),
            r"cmd\x09\x09\x3b \\path"
        );

        // Test with null bytes
        assert_eq!(escape_command_for_osc_633("a\x00b\x01c"), r"a\x00b\x01c");

        // Test all three special cases together
        assert_eq!(escape_command_for_osc_633("\\;\n"), r"\\\x3b\x0a");
    }

    #[test]
    fn osc_633_escaping_empty() {
        assert_eq!(escape_command_for_osc_633(""), "");
    }

    #[test]
    fn osc_633_escaping_unicode() {
        // Unicode characters should pass through unchanged
        assert_eq!(escape_command_for_osc_633("echo 你好"), "echo 你好");
        assert_eq!(escape_command_for_osc_633("café"), "café");
        assert_eq!(escape_command_for_osc_633("🦀"), "🦀");

        // But should still escape special chars
        assert_eq!(escape_command_for_osc_633("你好;世界"), r"你好\x3b世界");
    }
}
