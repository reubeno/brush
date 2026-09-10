use std::borrow::Cow;
use std::fmt::Write;
use std::io::Write as _;

use crate::term_detection;

/// Utility for integrating with terminal emulators.
pub(crate) struct TerminalIntegration {
    /// Info about the hosting terminal.
    term: term_detection::TerminalInfo,
}

#[allow(dead_code)]
impl TerminalIntegration {
    /// Starts terminal integration, emitting the sequence that announces it, and returns the
    /// utility the interactive loop reports events to.
    ///
    /// # Arguments
    ///
    /// * `term_info` - Information about the terminal capabilities.
    pub fn init(term_info: term_detection::TerminalInfo) -> std::io::Result<Self> {
        let integration = Self { term: term_info };
        Self::write(integration.initialize().as_ref())?;
        Ok(integration)
    }

    /// Returns a utility that integrates with nothing: it reports no capabilities, so every
    /// event handler below does nothing and every sequence it composes is empty. This is what
    /// a shell gets when integration is switched off, so it need not ask whether it has one.
    pub fn disabled() -> Self {
        Self {
            term: term_detection::TerminalInfo::default(),
        }
    }

    //
    // Event handlers: these are called at the points in the interactive loop that the
    // terminal wants to know about; each does what the event calls for. A terminal that
    // reports no support has nothing to say at any of them, so every handler is inert.
    //

    /// Called after a command line has been read and before it runs.
    ///
    /// # Arguments
    ///
    /// * `command` - The command that is about to be executed.
    pub fn on_pre_exec_command(&self, command: &str) -> std::io::Result<()> {
        Self::write(self.pre_exec_command(command).as_ref())
    }

    /// Called after a command has run.
    ///
    /// # Arguments
    ///
    /// * `exit_code` - The exit code the command left behind.
    pub fn on_post_exec_command(&self, exit_code: i32) -> std::io::Result<()> {
        Self::write(self.post_exec_command(exit_code).as_ref())
    }

    /// Writes a sequence to standard output, flushing so the terminal sees it before whatever
    /// the shell does next. Writing nothing still flushes; that costs nothing and keeps the
    /// inert case from being a separate path.
    fn write(seq: &str) -> std::io::Result<()> {
        let mut stdout = std::io::stdout();
        stdout.write_all(seq.as_bytes())?;
        stdout.flush()
    }

    /// Returns the terminal escape sequence that should be emitted to initialize terminal
    /// integration.
    pub fn initialize(&self) -> Cow<'_, str> {
        if self.term.supports_osc_633 {
            "\x1b]633;P;HasRichCommandDetection=True\x1b\\".into()
        } else {
            "".into()
        }
    }

    /// Returns a composed prompt bracketed with the sequences that mark where a prompt starts
    /// and ends, and that report the working directory -- or the prompt as given, when there is
    /// nothing to add. Returned rather than composed in place so the untouched prompt costs
    /// nothing, not even the copy that concatenating three empty strings onto it would make.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt as composed by the shell.
    /// * `working_dir` - The shell's current working directory.
    pub fn decorate_prompt(&self, prompt: String, working_dir: &std::path::Path) -> String {
        if !self.term.supports_osc_633 {
            return prompt;
        }

        [
            self.pre_prompt().as_ref(),
            self.report_cwd(working_dir).as_ref(),
            prompt.as_str(),
            self.post_prompt().as_ref(),
        ]
        .concat()
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
            let escaped_cwd_str = osc_633_escape(cwd.to_string_lossy().as_ref());
            format!("\x1b]633;P;Cwd={escaped_cwd_str}\x1b\\").into()
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
            let mut escaped_command = osc_633_escape(command);
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

    /// Returns the terminal escape sequence that should be emitted after the input line
    /// continuation.
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

/// Escapes a string for safe inclusion in an OSC 633 escape sequence.
/// Reference: <https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/terminal/common/scripts/shellIntegration-bash.sh>
fn osc_633_escape(command: &str) -> String {
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
    fn osc_633_escape_basic() {
        // Test simple alphanumeric string
        assert_eq!(osc_633_escape("echo hello"), "echo hello");
        assert_eq!(osc_633_escape("ls -la"), "ls -la");
    }

    #[test]
    fn osc_633_escape_semicolon() {
        // Semicolons should be escaped
        assert_eq!(osc_633_escape("cmd1; cmd2"), r"cmd1\x3b cmd2");
        assert_eq!(osc_633_escape(";"), r"\x3b");
        assert_eq!(osc_633_escape("a;b;c"), r"a\x3bb\x3bc");
    }

    #[test]
    fn osc_633_escape_backslash() {
        // Backslashes should be escaped
        assert_eq!(osc_633_escape(r"echo \n"), r"echo \\n");
        assert_eq!(osc_633_escape(r"\"), r"\\");
        assert_eq!(osc_633_escape(r"C:\path\to\file"), r"C:\\path\\to\\file");
    }

    #[test]
    fn osc_633_escape_control_chars() {
        // ASCII control characters (0x00-0x1e, i.e., 0-30) should be escaped
        assert_eq!(osc_633_escape("\x00"), r"\x00");
        assert_eq!(osc_633_escape("\x01"), r"\x01");
        assert_eq!(osc_633_escape("\t"), r"\x09"); // tab
        assert_eq!(osc_633_escape("\n"), r"\x0a"); // newline
        assert_eq!(osc_633_escape("\r"), r"\x0d"); // carriage return
        assert_eq!(osc_633_escape("\x1e"), r"\x1e"); // last control char (30)

        // 0x1f (31) should NOT be escaped as a control char (not < 31)
        assert_eq!(osc_633_escape("\x1f"), "\x1f");

        // Space (0x20, 32) should NOT be escaped
        assert_eq!(osc_633_escape(" "), " ");
    }

    #[test]
    fn osc_633_escape_mixed() {
        // Test combinations of different escape scenarios
        assert_eq!(
            osc_633_escape("echo\nhello; world\\n"),
            r"echo\x0ahello\x3b world\\n"
        );

        assert_eq!(osc_633_escape("cmd\t\t; \\path"), r"cmd\x09\x09\x3b \\path");

        // Test with null bytes
        assert_eq!(osc_633_escape("a\x00b\x01c"), r"a\x00b\x01c");

        // Test all three special cases together
        assert_eq!(osc_633_escape("\\;\n"), r"\\\x3b\x0a");
    }

    #[test]
    fn osc_633_escape_empty() {
        assert_eq!(osc_633_escape(""), "");
    }

    #[test]
    fn osc_633_escape_unicode() {
        // Unicode characters should pass through unchanged
        assert_eq!(osc_633_escape("echo 你好"), "echo 你好");
        assert_eq!(osc_633_escape("café"), "café");
        assert_eq!(osc_633_escape("🦀"), "🦀");

        // But should still escape special chars
        assert_eq!(osc_633_escape("你好;世界"), r"你好\x3b世界");
    }
}
