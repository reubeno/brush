use std::borrow::Cow;
use std::fmt::Write;

use crate::term;

/// Utility for integrating with terminal emulators.
#[derive(Default)]
pub(crate) struct TerminalIntegration {
    /// Info about the hosting terminal.
    term: term::TerminalInfo,
}

#[allow(dead_code)]
impl TerminalIntegration {
    /// Creates a new terminal integration utility.
    ///
    /// # Arguments
    ///
    /// * `term_info` - Information about the terminal capabilities.
    pub const fn new(term_info: term::TerminalInfo) -> Self {
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
            ';' => result.push_str(r"\x3b"),
            '\x00'..='\x1f' => {
                let _ = write!(result, r"\x{:02x}", c as u8);
            }
            _ => result.push(c),
        }
    }

    result
}
