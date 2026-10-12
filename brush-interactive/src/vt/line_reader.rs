//! Reading a program, a line at a time: keys in, edited lines out.

use std::io::Write;
use std::sync::Arc;

use brush_core::interfaces::InputFunction;
use tokio::sync::Mutex;

use super::completion_list;
use super::keymap::{Binding, VtKeyBindings};
use super::keys::{self, KEYSEQ_TIMEOUT, KeyReader, Keypress};
use super::line_editor::{Action, KillBuffer, LineEditor, SearchStep, is_kill};
use super::render::printable;
use super::style::Style;
use super::terminal::Terminal;
use crate::{
    ShellError, completeness,
    input_backend::{InteractivePrompt, ReadResult},
};

/// Width to assume for a terminal that doesn't report one.
const DEFAULT_COLUMNS: usize = 80;

/// How many macros may expand, one within another, before a key next comes from
/// the terminal, so a macro that (directly or not) expands to itself can't loop
/// forever.
const MAX_MACRO_EXPANSIONS: usize = 100;

/// Reads programs a line at a time, editing each line as keys arrive, and keeps
/// what lasts from one line to the next: key bindings, what's been killed, and
/// the line (and the program) a bound command sets aside.
#[derive(Default)]
pub(crate) struct LineReader {
    pub style: Style,
    pub bindings: Arc<Mutex<VtKeyBindings>>,
    /// The line being edited when a bound key was pressed, while its command runs.
    pub bound_line: Option<(String, usize)>,
    /// The line to start the next read with (e.g., as left by a bound command).
    pub next_line: Option<(String, usize)>,
    /// The lines of a program entered before a bound key was pressed, to go on
    /// with once its command has run.
    unfinished_program: String,
    /// Whether the line was erased for a bound command, to be drawn again in its
    /// place once the command has run.
    redraw_in_place: bool,
    keys: KeyReader,
    kills: KillBuffer,
    /// How many macros have expanded since a key last came from the terminal.
    macro_expansions: usize,
}

/// How reading a line ended.
enum LineResult {
    /// The user entered the line.
    Line(String),
    /// Input ended.
    Eof,
    /// The user interrupted input.
    Interrupted,
    /// The user pressed a key bound to the given shell command.
    Bound(String),
}

impl LineReader {
    /// Reads lines until they form a complete program, reading keys from
    /// `terminal` and drawing on `out`.
    pub async fn read_program(
        &mut self,
        shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: &InteractivePrompt,
        terminal: &mut impl Terminal,
        out: &mut (impl Write + Send),
    ) -> Result<ReadResult, ShellError> {
        // A program that a bound command interrupted goes on where it left off.
        let mut program = std::mem::take(&mut self.unfinished_program);
        let mut prompt_text = if program.is_empty() {
            prompt.prompt.as_str()
        } else {
            prompt.continuation_prompt.as_str()
        };

        loop {
            match self
                .read_one_line(shell, prompt_text, terminal, out)
                .await?
            {
                LineResult::Line(line) => {
                    program.push_str(&line);
                    program.push('\n');
                    if !completeness::needs_more_input(shell, &program) {
                        return Ok(ReadResult::Input(program));
                    }
                    prompt_text = prompt.continuation_prompt.as_str();
                }
                LineResult::Eof if program.is_empty() => return Ok(ReadResult::Eof),
                LineResult::Eof => return Ok(ReadResult::Input(program)),
                LineResult::Interrupted => return Ok(ReadResult::Interrupted),
                LineResult::Bound(command) => {
                    self.unfinished_program = program;
                    return Ok(ReadResult::BoundCommand(command));
                }
            }
        }
    }

    /// Reads and edits one line of input.
    async fn read_one_line(
        &mut self,
        shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: &str,
        terminal: &mut impl Terminal,
        out: &mut (impl Write + Send),
    ) -> Result<LineResult, ShellError> {
        // Only the prompt's last line is redrawn with the input; the rest is shown
        // once, and again only when the line is drawn afresh.
        let (prompt_above, prompt) = match prompt.rsplit_once('\n') {
            Some((above, last)) => (printable(above), last),
            None => (String::new(), prompt),
        };
        // The terminal's width is read once per line, not on every redraw: a line
        // being entered while the terminal is resized is drawn for the old width
        // until the next line.
        let columns = columns(terminal);
        let mut editor = LineEditor::new(prompt, prompt_above, self.style);
        // Output (e.g., from the last command) may not have ended with a newline.
        editor.start_on_own_row(out, columns)?;
        // A line erased for a bound command is drawn again in its place, below the
        // rest of the prompt, which is still shown.
        if !std::mem::take(&mut self.redraw_in_place) {
            editor.show_prompt_above(out)?;
        }
        if let Some((text, cursor)) = self.next_line.take() {
            editor.buffer.set(text, cursor);
        }
        editor.draw(out, columns)?;

        loop {
            let Some(key) = self.keys.next(terminal)? else {
                editor.finish(out, columns)?;
                return Ok(LineResult::Eof);
            };
            if !key.from_macro {
                self.macro_expansions = 0;
            }

            // As at a terminal's own line editing, whatever they're bound to, Ctrl-C
            // interrupts, and Ctrl-D on an empty line ends input.
            if key.bytes == [0x03] {
                // Leave the interrupted line as it was, marked as such.
                editor.buffer.end();
                editor.draw(out, columns)?;
                out.write_all(b"^C\n")?;
                return Ok(LineResult::Interrupted);
            }
            if key.bytes == [0x04] && editor.buffer.text().is_empty() && !editor.searching() {
                editor.finish(out, columns)?;
                return Ok(LineResult::Eof);
            }

            let Some(action) = self.resolve(key, terminal).await? else {
                // A macro expanded; its keys come next.
                continue;
            };

            // History is read from the shell as it's browsed or searched.
            let taken_by_search = editor.searching() && {
                let locked = shell.lock().await;
                editor.search_step(&action, &locked.history()) == SearchStep::Taken
            };
            if !taken_by_search {
                match &action {
                    Action::Command(command) => {
                        // The command runs with the line set aside for it.
                        self.redraw_in_place = editor.set_aside(out, columns)?;
                        self.bound_line =
                            Some((editor.buffer.text().to_owned(), editor.buffer.cursor()));
                        return Ok(LineResult::Bound(command.clone()));
                    }
                    Action::Function(InputFunction::Complete) => {
                        completion_list::complete(shell, &mut editor, out, columns, self.style)
                            .await?;
                    }
                    Action::Function(function) => {
                        let locked = shell.lock().await;
                        let accepted = editor.perform(
                            function,
                            &mut self.kills,
                            &locked.history(),
                            out,
                            columns,
                        )?;
                        drop(locked);
                        if let Some(line) = accepted {
                            editor.finish(out, columns)?;
                            return Ok(LineResult::Line(line));
                        }
                    }
                    Action::Insert(c) => editor.insert(*c),
                    Action::Unbound { .. } => {}
                }
            }

            self.kills.chained =
                !taken_by_search && matches!(&action, Action::Function(f) if is_kill(f));
            editor.completed_last =
                !taken_by_search && action == Action::Function(InputFunction::Complete);
            editor.draw(out, columns)?;
        }
    }

    /// Works out what a pressed key does, given the key bindings, reading more keys
    /// if it starts a bound sequence. Returns `None` if the keys were bound to a
    /// macro, whose keys are then the next to act on.
    async fn resolve(
        &mut self,
        first: Keypress,
        terminal: &mut impl Terminal,
    ) -> std::io::Result<Option<Action>> {
        let mut held = vec![first];
        // The longest bound sequence of keys held, and what it's bound to.
        let mut bound: Option<(usize, Binding)> = None;

        loop {
            let lookup = self.bindings.lock().await.lookup(&held);
            if let Some(binding) = lookup.bound {
                bound = Some((held.len(), binding));
            }
            if !lookup.prefix {
                break;
            }

            // A longer bound sequence starts with the keys held. If they're bound
            // themselves, wait only a little for the next, as readline does.
            let next = if bound.as_ref().is_some_and(|(len, _)| *len == held.len()) {
                self.keys.next_within(terminal, KEYSEQ_TIMEOUT)?
            } else {
                self.keys.next(terminal)?
            };
            let Some(next) = next else {
                break;
            };
            held.push(next);
        }

        // Keys held past the bound sequence (or past the first, if none is bound)
        // are acted on next.
        let used = bound.as_ref().map_or(1, |(len, _)| *len);
        let rest = held.split_off(used);
        self.keys.push_front(rest);
        let first = held.swap_remove(0);

        let action = match bound.map(|(_, binding)| binding) {
            Some(Binding::Command(command)) => Action::Command(command),
            Some(Binding::Function(InputFunction::SelfInsert)) | None => {
                first.typed_char().map_or_else(
                    || Action::Unbound {
                        escape: first.is_escape(),
                    },
                    Action::Insert,
                )
            }
            Some(Binding::Function(function)) => Action::Function(function),
            Some(Binding::Macro(bytes)) => {
                if self.macro_expansions < MAX_MACRO_EXPANSIONS {
                    self.macro_expansions += 1;
                    self.keys.push_front(keys::decode_all(&bytes));
                }
                return Ok(None);
            }
        };
        Ok(Some(action))
    }
}

/// Returns `terminal`'s width, or a usual width if it doesn't report one.
fn columns(terminal: &impl Terminal) -> usize {
    terminal
        .columns()
        .filter(|&c| c > 0)
        .unwrap_or(DEFAULT_COLUMNS)
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in tests")]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use brush_core::interfaces::{Key, KeyAction, KeyBindings as _, KeySequence, KeyStroke};

    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// What a test terminal sends, in turn.
    enum Chunk {
        /// Bytes, arriving together.
        Data(Vec<u8>),
        /// A pause: nothing more arrives while the reader waits.
        Pause,
        /// A read interrupted by a signal.
        Interrupted,
    }

    fn data(bytes: impl AsRef<[u8]>) -> Chunk {
        Chunk::Data(bytes.as_ref().to_vec())
    }

    /// A terminal that sends the given chunks, and reports a width.
    struct FakeTerminal {
        columns: Option<usize>,
        input: VecDeque<Chunk>,
    }

    impl FakeTerminal {
        fn new(input: impl IntoIterator<Item = Chunk>) -> Self {
            Self {
                columns: Some(80),
                input: input.into_iter().collect(),
            }
        }
    }

    impl Terminal for FakeTerminal {
        fn enable_raw_mode(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn disable_raw_mode(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn columns(&self) -> Option<usize> {
            self.columns
        }

        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            while let Some(chunk) = self.input.pop_front() {
                match chunk {
                    Chunk::Data(bytes) => {
                        let count = bytes.len().min(buf.len());
                        buf[..count].copy_from_slice(&bytes[..count]);
                        if count < bytes.len() {
                            self.input.push_front(Chunk::Data(bytes[count..].to_vec()));
                        }
                        return Ok(count);
                    }
                    Chunk::Pause => {}
                    Chunk::Interrupted => {
                        return Err(std::io::ErrorKind::Interrupted.into());
                    }
                }
            }
            Ok(0)
        }

        fn wait_for_input(&mut self, _timeout: Duration) -> std::io::Result<bool> {
            Ok(match self.input.front() {
                Some(Chunk::Pause) => {
                    self.input.pop_front();
                    false
                }
                Some(_) => true,
                None => false,
            })
        }
    }

    /// How reading a program ended, comparably.
    #[derive(Debug, PartialEq, Eq)]
    enum Outcome {
        Input(String),
        Bound(String),
        Eof,
        Interrupted,
    }

    impl From<ReadResult> for Outcome {
        fn from(result: ReadResult) -> Self {
            match result {
                ReadResult::Input(text) => Self::Input(text),
                ReadResult::BoundCommand(command) => Self::Bound(command),
                ReadResult::Eof => Self::Eof,
                ReadResult::Interrupted => Self::Interrupted,
            }
        }
    }

    fn input(text: &str) -> Outcome {
        Outcome::Input(text.to_owned())
    }

    fn reader() -> LineReader {
        LineReader::default()
    }

    fn basic_reader() -> LineReader {
        LineReader {
            style: Style::Basic,
            ..LineReader::default()
        }
    }

    async fn shell() -> crate::ShellRef {
        let shell = brush_core::Shell::builder()
            .profile(brush_core::ProfileLoadBehavior::Skip)
            .rc(brush_core::RcLoadBehavior::Skip)
            .build()
            .await
            .unwrap();
        Arc::new(Mutex::new(shell))
    }

    /// A shell with the given history, oldest first.
    async fn shell_with_history(entries: &[&str]) -> crate::ShellRef {
        let mut shell = brush_core::Shell::builder()
            .interactive(true)
            .profile(brush_core::ProfileLoadBehavior::Skip)
            .rc(brush_core::RcLoadBehavior::Skip)
            .build()
            .await
            .unwrap();
        for entry in entries {
            shell
                .history_mut()
                .unwrap()
                .add(brush_core::history::Item::new(*entry))
                .unwrap();
        }
        Arc::new(Mutex::new(shell))
    }

    /// A shell that completes the arguments of `cmd` from `words`.
    async fn shell_completing(words: &str) -> crate::ShellRef {
        let shell = shell().await;
        shell.lock().await.completion_config_mut().set(
            brush_core::completion::SpecName::Command("cmd"),
            brush_core::completion::Spec {
                word_list: Some(words.to_owned()),
                ..Default::default()
            },
        );
        shell
    }

    /// Reads a program from the terminal sending `chunks`, with the prompt
    /// `prompt`; returns how reading ended and what was drawn.
    async fn read_with(
        reader: &mut LineReader,
        shell: &crate::ShellRef,
        prompt: &str,
        chunks: Vec<Chunk>,
    ) -> (Outcome, String) {
        let prompt = InteractivePrompt {
            prompt: prompt.to_owned(),
            continuation_prompt: "> ".to_owned(),
            ..Default::default()
        };
        let mut terminal = FakeTerminal::new(chunks);
        let mut out = vec![];
        let result = reader
            .read_program(shell, &prompt, &mut terminal, &mut out)
            .await
            .unwrap();
        (result.into(), String::from_utf8(out).unwrap())
    }

    /// Reads a program from `keys`, with a `$ ` prompt.
    async fn read(
        reader: &mut LineReader,
        shell: &crate::ShellRef,
        keys: impl AsRef<[u8]>,
    ) -> (Outcome, String) {
        read_with(reader, shell, "$ ", vec![data(keys)]).await
    }

    /// Plays what was drawn on an 80-column terminal, translating newlines to CR
    /// LF as terminals do in raw mode; returns the rows it then shows, up to the
    /// last that isn't blank, and the cursor's row and column.
    fn shown(drawn: &str) -> (Vec<String>, (u16, u16)) {
        let mut terminal = vt100::Parser::new(24, 80, 0);
        terminal.process(drawn.replace('\n', "\r\n").as_bytes());
        let screen = terminal.screen();
        let mut rows: Vec<String> = screen
            .rows(0, 80)
            .map(|row| row.trim_end().to_owned())
            .collect();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        (rows, screen.cursor_position())
    }

    fn ctrl(c: char) -> KeyStroke {
        KeyStroke {
            control: true,
            ..Key::Character(c).into()
        }
    }

    async fn bind(reader: &LineReader, keys: Vec<KeyStroke>, action: KeyAction) {
        reader
            .bindings
            .lock()
            .await
            .bind(KeySequence::Strokes(keys), action)
            .unwrap();
    }

    async fn bind_command(reader: &LineReader, keys: Vec<KeyStroke>, command: &str) {
        bind(reader, keys, KeyAction::ShellCommand(command.to_owned())).await;
    }

    async fn define_macro(reader: &LineReader, keys: Vec<KeyStroke>, target: &[u8]) {
        reader
            .bindings
            .lock()
            .await
            .define_macro(
                KeySequence::Strokes(keys),
                KeySequence::Bytes(vec![target.to_vec()]),
            )
            .unwrap();
    }

    #[tokio::test]
    async fn reads_an_edited_line() {
        let (outcome, drawn) = read(&mut reader(), &shell().await, b"ech\x01x\x05o\r").await;
        assert_eq!(outcome, input("xecho\n"));
        assert_eq!(shown(&drawn), (strings(&["$ xecho"]), (1, 0)));
    }

    #[tokio::test]
    async fn reads_lines_until_the_program_is_complete() {
        let (outcome, drawn) = read(&mut reader(), &shell().await, b"if true\rthen :\rfi\r").await;
        assert_eq!(outcome, input("if true\nthen :\nfi\n"));
        assert_eq!(shown(&drawn).0, ["$ if true", "> then :", "> fi"]);
    }

    #[tokio::test]
    async fn ends_at_the_end_of_input() {
        let shell = shell().await;
        assert_eq!(read(&mut reader(), &shell, b"").await.0, Outcome::Eof);
        assert_eq!(read(&mut reader(), &shell, b"\x04").await.0, Outcome::Eof);
        // What's been entered of an incomplete program is still run.
        assert_eq!(
            read(&mut reader(), &shell, b"if true\r").await.0,
            input("if true\n")
        );
    }

    #[tokio::test]
    async fn ctrl_d_deletes_on_a_line_that_is_not_empty() {
        let (outcome, _) = read(&mut reader(), &shell().await, b"ab\x02\x04\r").await;
        assert_eq!(outcome, input("a\n"));
    }

    #[tokio::test]
    async fn interrupts_leaving_the_line_marked() {
        let (outcome, drawn) = read(&mut reader(), &shell().await, b"abc\x02\x02\x03").await;
        assert_eq!(outcome, Outcome::Interrupted);
        assert_eq!(shown(&drawn), (strings(&["$ abc^C"]), (1, 0)));
    }

    #[tokio::test]
    async fn shows_the_earlier_lines_of_a_prompt_once() {
        let prompt = "\x01\x1b[1m\x02info\x01\x1b[0m\x02\n$ ";
        let (outcome, drawn) =
            read_with(&mut reader(), &shell().await, prompt, vec![data(b"ab\r")]).await;
        assert_eq!(outcome, input("ab\n"));
        // Without the markers of non-printing text, once on a row of its own.
        let drawn = drawn.trim_start_matches([' ', '\r']);
        assert!(drawn.starts_with("\x1b[1minfo\x1b[0m\n"), "{drawn:?}");
        assert_eq!(drawn.matches("info").count(), 1, "{drawn:?}");
    }

    #[tokio::test]
    async fn shows_all_of_a_prompt_when_drawing_the_line_afresh() {
        let shell = shell_completing("alpha1 alpha2").await;
        let prompt = "info\n$ ";

        // After listing candidates.
        let (_, drawn) = read_with(
            &mut reader(),
            &shell,
            prompt,
            vec![data(b"cmd alpha\t\t\r")],
        )
        .await;
        assert_eq!(
            shown(&drawn).0,
            [
                "info",
                "$ cmd alpha",
                "alpha1  alpha2",
                "info",
                "$ cmd alpha"
            ]
        );

        // After clearing the screen.
        let (_, drawn) = read_with(&mut reader(), &shell, prompt, vec![data(b"x\x0c\r")]).await;
        assert_eq!(shown(&drawn).0, ["info", "$ x"]);
        let (_, drawn) =
            read_with(&mut basic_reader(), &shell, prompt, vec![data(b"x\x0c\r")]).await;
        assert!(!drawn.contains('\x1b'), "{drawn:?}");
        assert_eq!(drawn, "info\n$ x\ninfo\n$ x\n");
    }

    #[tokio::test]
    async fn shows_control_characters_in_caret_notation() {
        let shell = shell_with_history(&["for i in 1 2\ndo echo $i\ndone"]).await;

        // A multi-line history entry takes the row it's measured to.
        let (outcome, drawn) = read_with(&mut reader(), &shell, "$ ", vec![data(b"\x1b[A")]).await;
        assert_eq!(outcome, Outcome::Eof);
        assert_eq!(shown(&drawn).0, ["$ for i in 1 2^Jdo echo $i^Jdone"]);
        let (outcome, drawn) = read_with(
            &mut reader(),
            &shell,
            "$ ",
            vec![data(b"\x1b[A\x1b[B\x1b[A\x1b[Bx\r")],
        )
        .await;
        assert_eq!(outcome, input("x\n"));
        assert_eq!(shown(&drawn), (strings(&["$ x"]), (1, 0)));
        let (outcome, _) = read_with(&mut reader(), &shell, "$ ", vec![data(b"\x1b[A\r")]).await;
        assert_eq!(outcome, input("for i in 1 2\ndo echo $i\ndone\n"));

        // Escape sequences in the line are shown, not acted on by the terminal.
        let mut reader = reader();
        reader.next_line = Some(("a\tb\x1b[31mred".to_owned(), 3));
        let (_, drawn) = read(&mut reader, &shell, b"").await;
        assert_eq!(shown(&drawn).0, ["$ a^Ib^[[31mred"]);
        assert!(!drawn.contains("\x1b[31m"), "{drawn:?}");
    }

    #[tokio::test]
    async fn decodes_keys_split_across_reads() {
        let shell = shell().await;
        let (outcome, _) = read_with(
            &mut reader(),
            &shell,
            "$ ",
            vec![data(b"\xe7"), data(b"\x95\x8c"), data(b"\r")],
        )
        .await;
        assert_eq!(outcome, input("界\n"));

        // The rest of an escape sequence that arrives later is still part of it.
        let (outcome, _) = read_with(
            &mut reader(),
            &shell,
            "$ ",
            vec![data(b"ab"), data(b"\x1b"), data(b"[D"), data(b"x\r")],
        )
        .await;
        assert_eq!(outcome, input("axb\n"));

        // But after a pause, Escape is the Escape key, which does nothing, and what
        // follows is typed.
        let (outcome, _) = read_with(
            &mut reader(),
            &shell,
            "$ ",
            vec![data(b"ab\x1b"), Chunk::Pause, data(b"[Dx\r")],
        )
        .await;
        assert_eq!(outcome, input("ab[Dx\n"));
        let (outcome, _) = read_with(
            &mut reader(),
            &shell,
            "$ ",
            vec![data(b"ab\x1bO"), Chunk::Pause, data(b"x\r")],
        )
        .await;
        assert_eq!(outcome, input("abx\n"));
    }

    #[tokio::test]
    async fn doesnt_swallow_keys_after_alt_bracket() {
        let (outcome, _) = read(&mut reader(), &shell().await, b"ab\x1b[\rls\r").await;
        assert_eq!(outcome, input("ab\n"));
    }

    #[tokio::test]
    async fn retries_interrupted_reads() {
        let (outcome, _) = read_with(
            &mut reader(),
            &shell().await,
            "$ ",
            vec![Chunk::Interrupted, data(b"x\r")],
        )
        .await;
        assert_eq!(outcome, input("x\n"));
    }

    #[tokio::test]
    async fn starts_with_the_line_a_bound_command_left() {
        let mut reader = reader();
        reader.next_line = Some(("echo x".to_owned(), 4));
        let (outcome, _) = read(&mut reader, &shell().await, b"y\r").await;
        assert_eq!(outcome, input("echoy x\n"));
    }

    #[tokio::test]
    async fn runs_commands_bound_to_keys_with_the_line_set_aside() {
        let mut reader = reader();
        bind_command(&reader, vec![ctrl('x')], "cmd").await;
        let shell = shell().await;

        let (outcome, drawn) = read(&mut reader, &shell, b"ab\x02\x18").await;
        assert_eq!(outcome, Outcome::Bound("cmd".to_owned()));
        assert_eq!(reader.bound_line, Some(("ab".to_owned(), 1)));
        assert_eq!(shown(&drawn), (vec![], (0, 0)));
    }

    #[tokio::test]
    async fn redraws_the_line_in_place_after_a_bound_command() {
        let shell = shell().await;
        let prompt = "info\n$ ";

        // The line's erased while the command runs, then drawn again where it was,
        // below what the command wrote, if anything, and the rest of the prompt.
        for (written, rows) in [
            ("", vec!["info", "$ ab"]),
            ("out\n", vec!["info", "out", "$ ab"]),
        ] {
            let mut reader = reader();
            bind_command(&reader, vec![ctrl('x')], "cmd").await;
            let (outcome, mut drawn) =
                read_with(&mut reader, &shell, prompt, vec![data(b"ab\x18")]).await;
            assert_eq!(outcome, Outcome::Bound("cmd".to_owned()));

            drawn.push_str(written);
            reader.next_line = reader.bound_line.take();
            let (outcome, redrawn) =
                read_with(&mut reader, &shell, prompt, vec![data(b"\r")]).await;
            drawn.push_str(&redrawn);
            assert_eq!(outcome, input("ab\n"));
            assert_eq!(shown(&drawn).0, rows);
        }

        // Without cursor addressing, the line's left as it was, and drawn afresh
        // below.
        let mut reader = basic_reader();
        bind_command(&reader, vec![ctrl('x')], "cmd").await;
        let (_, mut drawn) = read_with(&mut reader, &shell, prompt, vec![data(b"ab\x18")]).await;
        reader.next_line = reader.bound_line.take();
        let (_, redrawn) = read_with(&mut reader, &shell, prompt, vec![data(b"\r")]).await;
        drawn.push_str(&redrawn);
        assert_eq!(drawn, "info\n$ ab\ninfo\n$ ab\n");
    }

    #[tokio::test]
    async fn runs_bound_commands_on_continuation_lines() {
        let mut reader = reader();
        bind_command(&reader, vec![ctrl('x')], "cmd").await;
        let shell = shell().await;

        // The command sees just the line it was run from, and the program goes on
        // from there once it's run.
        let (outcome, mut drawn) = read(&mut reader, &shell, b"if true; then\rf\x18").await;
        assert_eq!(outcome, Outcome::Bound("cmd".to_owned()));
        assert_eq!(reader.bound_line, Some(("f".to_owned(), 1)));

        reader.next_line = Some(("fi".to_owned(), 2));
        let (outcome, redrawn) = read(&mut reader, &shell, b"\r").await;
        drawn.push_str(&redrawn);
        assert_eq!(outcome, input("if true; then\nfi\n"));
        assert_eq!(shown(&drawn).0, ["$ if true; then", "> fi"]);

        // What follows is a new program.
        let (outcome, _) = read(&mut reader, &shell, b"x\r").await;
        assert_eq!(outcome, input("x\n"));
    }

    #[tokio::test]
    async fn bound_commands_end_a_search_and_run() {
        let mut reader = reader();
        bind_command(&reader, vec![ctrl('x')], "cmd").await;
        let (outcome, _) = read_with(
            &mut reader,
            &shell_with_history(&["echo one"]).await,
            "$ ",
            vec![data(b"\x12one\x18")],
        )
        .await;
        assert_eq!(outcome, Outcome::Bound("cmd".to_owned()));
        assert_eq!(reader.bound_line, Some(("echo one".to_owned(), 5)));
    }

    #[tokio::test]
    async fn performs_functions_bound_to_keys() {
        let mut reader = reader();
        bind(
            &reader,
            vec![ctrl('x')],
            KeyAction::DoInputFunction(InputFunction::BeginningOfLine),
        )
        .await;
        // A function this backend doesn't perform leaves the key doing nothing.
        bind(
            &reader,
            vec![ctrl('t')],
            KeyAction::DoInputFunction(InputFunction::Undo),
        )
        .await;
        // A key bound to self-insert types itself.
        bind(
            &reader,
            vec![ctrl('v')],
            KeyAction::DoInputFunction(InputFunction::SelfInsert),
        )
        .await;

        let (outcome, _) = read(&mut reader, &shell().await, b"bc\x18a\x05\x14\x16\r").await;
        assert_eq!(outcome, input("abc\n"));
    }

    #[tokio::test]
    async fn binds_keys_that_send_the_same_bytes_as_others() {
        let mut reader = reader();
        // `bind '"\C-]": ...'`, and a key readline knows as Ctrl-H, not Backspace.
        bind_command(&reader, vec![ctrl(']')], "bracket").await;
        bind(
            &reader,
            vec![ctrl('h')],
            KeyAction::DoInputFunction(InputFunction::BeginningOfLine),
        )
        .await;
        let shell = shell().await;

        let (outcome, _) = read(&mut reader, &shell, b"\x1d").await;
        assert_eq!(outcome, Outcome::Bound("bracket".to_owned()));
        let (outcome, _) = read(&mut reader, &shell, b"bc\x08a\x7f\r").await;
        assert_eq!(outcome, input("bc\n"));
    }

    #[tokio::test]
    async fn expands_macros() {
        let mut reader = reader();
        define_macro(&reader, vec![ctrl('x')], b"hi").await;
        // A macro of keys that send escape sequences.
        define_macro(&reader, vec![ctrl('o')], b"\x1b[H").await;

        let (outcome, _) = read(&mut reader, &shell().await, b"\x18 there\x0f>\r").await;
        assert_eq!(outcome, input(">hi there\n"));
    }

    #[tokio::test]
    async fn stops_expanding_a_macro_that_expands_to_itself() {
        let mut reader = reader();
        define_macro(&reader, vec![ctrl('x')], b"\x18").await;
        let (outcome, _) = read(&mut reader, &shell().await, b"\x18a\r").await;
        assert_eq!(outcome, input("a\n"));
    }

    #[tokio::test]
    async fn limits_how_deep_macros_expand_not_how_often() {
        let mut reader = reader();
        define_macro(&reader, vec![ctrl('x')], b"a").await;
        let keys = [vec![0x18; 150], b"\r".to_vec()].concat();
        let (outcome, _) = read(&mut reader, &shell().await, keys).await;
        assert_eq!(outcome, input(&format!("{}\n", "a".repeat(150))));
    }

    #[tokio::test]
    async fn waits_for_the_rest_of_a_bound_sequence() {
        let mut reader = reader();
        reader
            .bindings
            .lock()
            .await
            .bind(
                KeySequence::Bytes(vec![b"\x18".to_vec(), b"e".to_vec()]),
                KeyAction::ShellCommand("edit".to_owned()),
            )
            .unwrap();
        reader
            .bindings
            .lock()
            .await
            .bind(
                KeySequence::Bytes(vec![b"\x18".to_vec(), b"\x18".to_vec(), b"x".to_vec()]),
                KeyAction::ShellCommand("long".to_owned()),
            )
            .unwrap();
        let shell = shell().await;

        let (outcome, _) = read(&mut reader, &shell, b"\x18e").await;
        assert_eq!(outcome, Outcome::Bound("edit".to_owned()));

        // Keys that turn out not to be a bound sequence take effect as usual.
        let (outcome, _) = read(&mut reader, &shell, b"\x18z\r").await;
        assert_eq!(outcome, input("z\n"));

        // When a longer sequence falls through, what follows its first key may still
        // be bound.
        let (outcome, _) = read(&mut reader, &shell, b"\x18\x18e").await;
        assert_eq!(outcome, Outcome::Bound("edit".to_owned()));
    }

    #[tokio::test]
    async fn waits_a_little_when_bound_keys_also_start_a_longer_sequence() {
        let mut reader = reader();
        bind_command(&reader, vec![ctrl('x')], "short").await;
        bind_command(&reader, vec![ctrl('x'), ctrl('e')], "long").await;
        let shell = shell().await;

        let read_chunks =
            async |reader: &mut LineReader, chunks| read_with(reader, &shell, "$ ", chunks).await.0;
        assert_eq!(
            read_chunks(&mut reader, vec![data(b"\x18\x05")]).await,
            Outcome::Bound("long".to_owned())
        );
        assert_eq!(
            read_chunks(&mut reader, vec![data(b"\x18"), data(b"\x05")]).await,
            Outcome::Bound("long".to_owned())
        );
        assert_eq!(
            read_chunks(&mut reader, vec![data(b"\x18"), Chunk::Pause]).await,
            Outcome::Bound("short".to_owned())
        );
        // A key that doesn't continue the sequence comes after the shorter one's
        // command.
        assert_eq!(
            read_chunks(&mut reader, vec![data(b"\x18z\r")]).await,
            Outcome::Bound("short".to_owned())
        );
    }

    #[tokio::test]
    async fn completes_a_single_candidate() {
        let shell = shell_completing("alpha beta").await;
        let (outcome, _) = read(&mut reader(), &shell, b"cmd be\t\r").await;
        assert_eq!(outcome, input("cmd beta \n"));
    }

    #[tokio::test]
    async fn extends_ambiguous_words_then_lists_candidates() {
        let shell = shell_completing("alpha1 alpha2 beta").await;

        let (outcome, drawn) = read(&mut reader(), &shell, b"cmd a\t\r").await;
        assert_eq!(outcome, input("cmd alpha\n"));
        assert!(!drawn.contains('\x07'), "{drawn:?}");

        // With nothing to extend, the first Tab rings the bell and a second lists the
        // candidates, below the line, which is then drawn again.
        let (outcome, drawn) = read(&mut reader(), &shell, b"cmd alpha\t\t\r").await;
        assert_eq!(outcome, input("cmd alpha\n"));
        assert_eq!(drawn.matches('\x07').count(), 1, "{drawn:?}");
        assert_eq!(
            shown(&drawn).0,
            ["$ cmd alpha", "alpha1  alpha2", "$ cmd alpha"]
        );

        // Only a second Tab in a row lists them.
        let (_, drawn) = read(&mut reader(), &shell, b"cmd alpha\tx\x7f\t\r").await;
        assert!(!drawn.contains("alpha1"), "{drawn:?}");
    }

    #[tokio::test]
    async fn lists_ambiguous_candidates_straight_away_in_basic_mode() {
        let shell = shell_completing("alpha1 alpha2 beta").await;
        let (outcome, drawn) = read(&mut basic_reader(), &shell, b"cmd a\t\r").await;
        assert_eq!(outcome, input("cmd a\n"));
        assert_eq!(drawn, "$ cmd a\nalpha1\nalpha2\n$ cmd a\n");
    }

    #[tokio::test]
    async fn rings_the_bell_when_nothing_completes() {
        let shell = shell_completing("alpha").await;

        let (outcome, drawn) = read(&mut reader(), &shell, b"cmd z\t\r").await;
        assert_eq!(outcome, input("cmd z\n"));
        assert!(drawn.contains('\x07'), "{drawn:?}");

        let (_, drawn) = read(&mut basic_reader(), &shell, b"cmd z\t\r").await;
        assert!(!drawn.contains('\x07'), "{drawn:?}");
    }

    #[tokio::test]
    async fn keeps_what_was_killed_from_line_to_line() {
        let mut reader = reader();
        let shell = shell().await;
        read(&mut reader, &shell, b"echo word\x17\r").await;
        let (outcome, _) = read(&mut reader, &shell, b"\x19\r").await;
        assert_eq!(outcome, input("word\n"));
    }

    #[test]
    fn assumes_a_width_when_the_terminal_reports_none() {
        let mut terminal = FakeTerminal::new([]);
        for (reported, assumed) in [
            (Some(120), 120),
            (Some(0), DEFAULT_COLUMNS),
            (None, DEFAULT_COLUMNS),
        ] {
            terminal.columns = reported;
            assert_eq!(columns(&terminal), assumed);
        }
    }

    #[tokio::test]
    async fn edits_a_line_of_multibyte_characters() {
        let (outcome, drawn) = read(&mut reader(), &shell().await, "é界\x02\x02x\x05!\r").await;
        assert_eq!(outcome, input("xé界!\n"));
        assert_eq!(shown(&drawn), (strings(&["$ xé界!"]), (1, 0)));
    }

    #[tokio::test]
    async fn searches_history_for_multibyte_text() {
        let (outcome, drawn) = read_with(
            &mut reader(),
            &shell_with_history(&["échec", "naïve café", "ls"]).await,
            "$ ",
            vec![data("\x12café\x02!\r")],
        )
        .await;
        // The match is shown with the cursor at its start; Ctrl-B ends the search
        // and moves back a character from there.
        assert_eq!(outcome, input("naïve! café\n"));
        assert!(
            drawn.contains("(reverse-i-search)`café': naïve café"),
            "{drawn:?}"
        );
    }

    #[tokio::test]
    async fn completes_multibyte_words() {
        let shell = shell_completing("été étage 日本語").await;
        let read_keys = async |keys: &str| read(&mut reader(), &shell, keys).await.0;

        // Candidates share `ét`, so Tab extends the word that far.
        assert_eq!(read_keys("cmd é\t\r").await, input("cmd ét\n"));
        assert_eq!(read_keys("cmd éta\t\r").await, input("cmd étage \n"));
        assert_eq!(read_keys("cmd 日\t\r").await, input("cmd 日本語 \n"));
    }
}
