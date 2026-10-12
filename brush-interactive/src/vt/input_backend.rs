use super::line_reader::LineReader;
use super::style::Style;
use super::terminal::{RawMode, StdTerminal, Terminal};
use crate::{
    InputBackend, ShellError,
    input_backend::{InteractivePrompt, ReadResult},
};

/// An input backend with readline-style line editing, history, and completion.
///
/// It talks to the terminal purely through bytes: it decodes keys from standard
/// input and draws on standard error, with VT100-style escape sequences or, in
/// the basic style ([`Self::with_basic_style`]), with backspaces alone. It needs
/// no terminal library, so it works wherever the terminal understands those
/// sequences, including hosts where the shell can't control the terminal through
/// the platform and instead supplies a [`Terminal`] of its own.
///
/// Its default terminal, [`StdTerminal`], works only on Unix: the Windows console
/// sends keys as bytes only in a mode that it doesn't set, and WASI has no raw
/// mode. Elsewhere, reading a line from a terminal fails, unless the host
/// supplies a [`Terminal`] that can switch to raw mode.
///
/// Key bindings start as readline's Emacs-mode defaults, and can be changed with
/// `bind`: keys and sequences of keys can be bound to readline functions (those
/// it performs), to shell commands (`bind -x`, with the line being edited
/// available in `READLINE_LINE` and `READLINE_POINT`), and to macros; see
/// [`Self::install_key_bindings`].
pub struct VtInputBackend<T: Terminal = StdTerminal> {
    terminal: T,
    reader: LineReader,
}

impl Default for VtInputBackend<StdTerminal> {
    fn default() -> Self {
        Self::new(StdTerminal::default())
    }
}

impl<T: Terminal> VtInputBackend<T> {
    /// Returns a new backend that edits on the given terminal.
    ///
    /// # Arguments
    ///
    /// * `terminal` - The terminal to edit on.
    pub fn new(terminal: T) -> Self {
        Self {
            terminal,
            reader: LineReader::default(),
        }
    }

    /// Has this backend draw and respond to Tab in a way that's simple for
    /// programs that drive the shell (e.g., test harnesses) to follow: the line is
    /// drawn with backspaces alone, so a line that wraps across rows isn't redrawn
    /// correctly; ambiguous completions are listed straight away, one per line;
    /// and the bell never rings.
    #[must_use]
    pub const fn with_basic_style(mut self) -> Self {
        self.reader.style = Style::Basic;
        self
    }

    /// Makes this backend the keeper of `shell`'s key bindings, so that bindings
    /// made with `bind` (e.g., in startup files) take effect while it reads input.
    /// Call this before running any of the shell's startup files.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell whose key bindings this backend should keep.
    pub fn install_key_bindings(
        &self,
        shell: &mut brush_core::Shell<impl brush_core::ShellExtensions>,
    ) {
        shell.set_key_bindings(Some(self.reader.bindings.clone()));
    }
}

impl<T: Terminal> InputBackend for VtInputBackend<T> {
    async fn read_line(
        &mut self,
        shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        self.reader.bound_line = None;

        // Without a terminal, there's nothing to edit on; just read lines.
        if !self.terminal.is_terminal() {
            return crate::MinimalInputBackend.read_line(shell, prompt).await;
        }

        let mut terminal = RawMode::enable(&mut self.terminal)?;
        let result = self
            .reader
            .read_program(shell, &prompt, &mut *terminal, &mut std::io::stderr())
            .await;
        let restored = terminal.restore();

        let result = result?;
        restored?;
        Ok(result)
    }

    fn get_read_buffer(&self) -> Option<(String, usize)> {
        self.reader.bound_line.clone()
    }

    fn set_read_buffer(&mut self, buffer: String, cursor: usize) {
        self.reader.next_line = Some((buffer, cursor));
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;
    use std::sync::{Arc, PoisonError};
    use std::task::{Context, Waker};

    use brush_core::interfaces::{Key, KeyAction, KeyBindings as _, KeySequence, KeyStroke};
    use tokio::sync::Mutex;

    use super::*;

    async fn shell() -> crate::ShellRef {
        let shell = brush_core::Shell::builder()
            .profile(brush_core::ProfileLoadBehavior::Skip)
            .rc(brush_core::RcLoadBehavior::Skip)
            .build()
            .await
            .unwrap();
        Arc::new(Mutex::new(shell))
    }

    /// A terminal that sends the given input, and logs how its mode is switched.
    struct LoggingTerminal {
        input: Vec<u8>,
        log: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl Terminal for LoggingTerminal {
        fn enable_raw_mode(&mut self) -> std::io::Result<()> {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push("enable");
            Ok(())
        }

        fn disable_raw_mode(&mut self) -> std::io::Result<()> {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push("disable");
            Ok(())
        }

        fn columns(&self) -> Option<usize> {
            Some(80)
        }

        fn is_terminal(&self) -> bool {
            true
        }

        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let count = self.input.len().min(buf.len());
            buf[..count].copy_from_slice(&self.input[..count]);
            self.input.drain(..count);
            Ok(count)
        }
    }

    #[tokio::test]
    async fn restores_the_terminal_when_reading_is_cancelled() {
        let shell = shell().await;
        let log = Arc::default();
        let mut backend = VtInputBackend::new(LoggingTerminal {
            input: b"\x01".to_vec(),
            log: Arc::clone(&log),
        });

        // With the shell held, reading stops at Ctrl-A, which reads the history;
        // then it's cancelled.
        {
            let _locked = shell.lock().await;
            let read = pin!(backend.read_line(&shell, InteractivePrompt::default()));
            let mut context = Context::from_waker(Waker::noop());
            assert!(read.poll(&mut context).is_pending());
        }
        assert_eq!(*log.lock().unwrap(), ["enable", "disable"]);

        // The backend reads as usual afterwards.
        backend.terminal.input = b"x\r".to_vec();
        let result = backend
            .read_line(&shell, InteractivePrompt::default())
            .await
            .unwrap();
        assert!(matches!(result, ReadResult::Input(text) if text == "x\n"));
        assert_eq!(
            *log.lock().unwrap(),
            ["enable", "disable", "enable", "disable"]
        );
    }

    #[test]
    fn edits_the_terminal_by_default() {
        let backend = VtInputBackend::default();
        assert_eq!(backend.reader.style, Style::Vt);
        assert_eq!(backend.with_basic_style().reader.style, Style::Basic);
    }

    #[test]
    fn hands_the_line_to_and_from_bound_commands() {
        let mut backend = VtInputBackend::default();
        assert_eq!(backend.get_read_buffer(), None);
        backend.reader.bound_line = Some(("ab".to_owned(), 1));
        assert_eq!(backend.get_read_buffer(), Some(("ab".to_owned(), 1)));
        backend.set_read_buffer("cd".to_owned(), 2);
        assert_eq!(backend.reader.next_line, Some(("cd".to_owned(), 2)));
    }

    #[tokio::test]
    async fn keeps_the_shell_key_bindings() {
        let shell = shell().await;
        let backend = VtInputBackend::default();
        backend.install_key_bindings(&mut *shell.lock().await);

        let ctrl_x = KeySequence::Strokes(vec![KeyStroke {
            control: true,
            ..Key::Character('x').into()
        }]);
        let bindings = shell.lock().await.key_bindings().cloned().unwrap();
        bindings
            .lock()
            .await
            .bind(ctrl_x.clone(), KeyAction::ShellCommand("cmd".to_owned()))
            .unwrap();
        assert_eq!(
            backend
                .reader
                .bindings
                .lock()
                .await
                .get_current()
                .get(&ctrl_x),
            Some(&KeyAction::ShellCommand("cmd".to_owned()))
        );
    }
}
