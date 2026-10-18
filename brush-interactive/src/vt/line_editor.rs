//! Editing a line: readline's functions, history, and incremental search.

use std::io::Write;

use brush_core::interfaces::InputFunction;

use super::line_buffer::LineBuffer;
use super::render::Renderer;
use super::style::Style;

/// What a key does, once key bindings are taken into account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Perform a readline function.
    Function(InputFunction),
    /// Insert a char (e.g., for an unbound key that types one).
    Insert(char),
    /// Run a shell command (bound with `bind -x`).
    Command(String),
    /// Nothing: an unbound key that doesn't type a char. Whether it's the Escape
    /// key matters while searching, which it ends.
    Unbound {
        /// Whether it's the Escape key on its own.
        escape: bool,
    },
}

/// The history a [`LineEditor`] browses and searches: entries numbered from 0,
/// the oldest. It's consulted only as history is browsed or searched, an entry at
/// a time, rather than copied for each line.
pub(crate) trait History {
    /// Returns how many entries there are.
    fn len(&self) -> usize;

    /// Returns the given entry, if there is one.
    fn get(&self, index: usize) -> Option<&str>;
}

/// History that may be missing (as a shell that keeps none has), and is then
/// empty.
impl<T: History + ?Sized> History for Option<&T> {
    fn len(&self) -> usize {
        self.map_or(0, |history| history.len())
    }

    fn get(&self, index: usize) -> Option<&str> {
        self.and_then(|history| history.get(index))
    }
}

#[cfg(test)]
impl History for [String] {
    fn len(&self) -> usize {
        <[String]>::len(self)
    }

    fn get(&self, index: usize) -> Option<&str> {
        <[String]>::get(self, index).map(String::as_str)
    }
}

impl History for brush_core::history::History {
    fn len(&self) -> usize {
        self.count()
    }

    fn get(&self, index: usize) -> Option<&str> {
        Self::get(self, index).map(|item| item.command_line.as_str())
    }
}

/// What a kill command most recently deleted, to be yanked back.
#[derive(Default)]
pub(crate) struct KillBuffer {
    text: String,
    /// Whether the last action killed text, so that a kill now adds to what it
    /// killed, as consecutive kills do in readline.
    pub chained: bool,
}

impl KillBuffer {
    /// Keeps `killed`, which was deleted before the cursor if `backward`, or after
    /// it otherwise.
    fn kill(&mut self, killed: &str, backward: bool) {
        // Killing nothing keeps what was killed before.
        if killed.is_empty() {
            return;
        }
        if !self.chained {
            self.text.clear();
        }
        if backward {
            self.text.insert_str(0, killed);
        } else {
            self.text.push_str(killed);
        }
    }
}

/// Whether `function` kills text (so that another kill straight after it adds to
/// what it killed).
pub(crate) const fn is_kill(function: &InputFunction) -> bool {
    matches!(
        function,
        InputFunction::KillLine
            | InputFunction::UnixLineDiscard
            | InputFunction::UnixWordRubout
            | InputFunction::BackwardKillWord
            | InputFunction::KillWord
    )
}

/// The state of editing one line.
pub(crate) struct LineEditor<'a> {
    /// The prompt's last line, drawn with the line being edited.
    prompt: &'a str,
    /// The prompt's lines before its last, if any, as written to the terminal.
    prompt_above: String,
    pub buffer: LineBuffer,
    /// Which history entry is shown; `None` for the line being entered.
    history_index: Option<usize>,
    /// The line being entered, set aside while a history entry is shown.
    draft: Option<LineBuffer>,
    search: Option<Search>,
    renderer: Renderer,
    style: Style,
    /// Whether the last action was completion (so another lists the candidates).
    pub completed_last: bool,
}

/// The state of an incremental history search (Ctrl-R).
struct Search {
    query: String,
    /// The history entry last found, if there is one.
    found: Option<usize>,
    /// Whether the query, as it is, matches nothing more; what was last found is
    /// still shown.
    failed: bool,
    /// What to go back to if the search is cancelled.
    original: LineBuffer,
    original_index: Option<usize>,
}

/// What became of a search, given an action.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SearchStep {
    /// The search took the action.
    Taken,
    /// The search ended, keeping what it found; the action is to take effect as
    /// usual.
    Ended,
}

impl<'a> LineEditor<'a> {
    /// Returns an editor for a new line.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt's last line, drawn with the line.
    /// * `prompt_above` - The prompt's earlier lines, as written to the terminal
    ///   (shown again when the screen is cleared or candidates listed).
    /// * `style` - How to draw.
    pub fn new(prompt: &'a str, prompt_above: String, style: Style) -> Self {
        Self {
            prompt,
            prompt_above,
            buffer: LineBuffer::default(),
            history_index: None,
            draft: None,
            search: None,
            renderer: Renderer::new(style),
            style,
            completed_last: false,
        }
    }

    /// Rings the bell, if the style rings it.
    pub fn bell(&self, out: &mut impl Write) -> std::io::Result<()> {
        if self.style == Style::Vt {
            out.write_all(b"\x07")?;
        }
        Ok(())
    }

    pub const fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// Refreshes semantic highlighting for the current buffer and cursor.
    pub fn highlight(
        &mut self,
        shell: &brush_core::Shell<impl brush_core::ShellExtensions>,
        program: &str,
    ) {
        let line = if program.is_empty() {
            std::borrow::Cow::Borrowed(self.buffer.text())
        } else {
            std::borrow::Cow::Owned(format!("{program}{}", self.buffer.text()))
        };
        let highlighted = crate::highlighting::highlight_command(
            shell,
            &line,
            program.len() + self.buffer.cursor(),
        );
        self.renderer.set_highlighting(&highlighted, program.len());
    }

    /// Draws the line, replacing what was drawn before.
    pub fn draw(&mut self, out: &mut impl Write, columns: usize) -> std::io::Result<()> {
        let prompt = self.search_prompt();
        let prompt = prompt.as_deref().unwrap_or(self.prompt);
        self.renderer.draw(
            out,
            prompt,
            self.buffer.text(),
            self.buffer.cursor(),
            columns,
        )
    }

    /// Draws the line a last time, and moves to the next.
    pub fn finish(&mut self, out: &mut impl Write, columns: usize) -> std::io::Result<()> {
        let prompt = self.search_prompt();
        let prompt = prompt.as_deref().unwrap_or(self.prompt);
        self.renderer
            .finish(out, prompt, self.buffer.text(), columns)
    }

    /// Takes the line off the terminal while a bound command runs. As bash does, it's
    /// erased, to be drawn again where it was; returns `true`. Without cursor
    /// addressing, there's no erasing it, so it's left in place and the next line
    /// is drawn afresh below; returns `false`.
    pub fn set_aside(&mut self, out: &mut impl Write, columns: usize) -> std::io::Result<bool> {
        if let Renderer::Vt(renderer) = &mut self.renderer {
            renderer.erase(out)?;
            Ok(true)
        } else {
            self.finish(out, columns)?;
            Ok(false)
        }
    }

    /// Moves to the start of a row of its own to start drawing, so the line doesn't
    /// overwrite output that didn't end with a newline.
    pub fn start_on_own_row(&self, out: &mut impl Write, columns: usize) -> std::io::Result<()> {
        self.renderer.start_on_own_row(out, columns)
    }

    /// Writes the prompt's lines before its last, to start drawing it afresh.
    pub fn show_prompt_above(&self, out: &mut impl Write) -> std::io::Result<()> {
        if !self.prompt_above.is_empty() {
            out.write_all(self.prompt_above.as_bytes())?;
            out.write_all(b"\n")?;
        }
        Ok(())
    }

    /// While searching, the prompt shows the search instead.
    fn search_prompt(&self) -> Option<String> {
        self.search.as_ref().map(|search| {
            let failed = if search.failed { "failed " } else { "" };
            std::format!("({failed}reverse-i-search)`{}': ", search.query)
        })
    }

    /// Inserts `c` at the cursor.
    pub fn insert(&mut self, c: char) {
        self.buffer.insert(c.encode_utf8(&mut [0; 4]));
    }

    /// Performs a readline function. Returns the line, if the function accepts it.
    /// Functions this editor doesn't perform do nothing.
    ///
    /// # Arguments
    ///
    /// * `function` - The function to perform.
    /// * `kills` - What's been killed, to add to or yank.
    /// * `history` - The history to browse and search.
    /// * `out` - The terminal.
    /// * `columns` - The terminal's width.
    pub fn perform(
        &mut self,
        function: &InputFunction,
        kills: &mut KillBuffer,
        history: &(impl History + ?Sized),
        out: &mut impl Write,
        columns: usize,
    ) -> std::io::Result<Option<String>> {
        use InputFunction as F;

        let b = &mut self.buffer;
        match function {
            F::AcceptLine => return Ok(Some(b.text().to_owned())),
            F::BeginningOfLine => b.home(),
            F::EndOfLine => b.end(),
            F::BackwardChar => b.left(),
            F::ForwardChar => b.right(),
            F::BackwardWord => b.word_left(),
            F::ForwardWord => b.word_right(),
            F::BackwardDeleteChar => b.backspace(),
            F::DeleteChar => b.delete(),
            F::KillLine => kills.kill(&b.kill_to_end(), false),
            F::UnixLineDiscard => kills.kill(&b.kill_to_start(), true),
            F::UnixWordRubout => kills.kill(&b.kill_big_word_before(), true),
            F::BackwardKillWord => kills.kill(&b.kill_word_before(), true),
            F::KillWord => kills.kill(&b.kill_word_after(), false),
            F::Yank => b.insert(&kills.text),
            F::TransposeChars => b.transpose(),
            F::PreviousHistory => self.history_previous(history, out)?,
            F::NextHistory => self.history_next(history, out)?,
            F::BeginningOfHistory => {
                let oldest = (history.len() > 0).then_some(0);
                self.show_history_entry(oldest, history);
            }
            F::EndOfHistory => self.show_history_entry(None, history),
            F::ReverseSearchHistory => self.start_search(),
            F::ClearScreen => self.clear_screen(out, columns)?,
            F::Abort => self.bell(out)?,
            _ => {}
        }
        Ok(None)
    }

    /// Clears the screen, leaving the line to be drawn at the top. Without cursor
    /// addressing there's no clearing it, so, as readline does, the line is drawn
    /// afresh on the next.
    fn clear_screen(&mut self, out: &mut impl Write, columns: usize) -> std::io::Result<()> {
        match self.style {
            Style::Vt => {
                out.write_all(b"\x1b[H\x1b[2J")?;
                self.renderer.reset();
            }
            Style::Basic => self.finish(out, columns)?,
        }
        self.show_prompt_above(out)
    }

    fn history_previous(
        &mut self,
        history: &(impl History + ?Sized),
        out: &mut impl Write,
    ) -> std::io::Result<()> {
        let previous = self
            .history_index
            .unwrap_or_else(|| history.len())
            .checked_sub(1);
        if previous.is_some() {
            self.show_history_entry(previous, history);
            Ok(())
        } else {
            self.bell(out)
        }
    }

    fn history_next(
        &mut self,
        history: &(impl History + ?Sized),
        out: &mut impl Write,
    ) -> std::io::Result<()> {
        if let Some(index) = self.history_index {
            let next = Some(index + 1).filter(|&next| next < history.len());
            self.show_history_entry(next, history);
            Ok(())
        } else {
            self.bell(out)
        }
    }

    /// Shows the given history entry (or, for `None`, the line being entered),
    /// setting that line aside whenever it's left.
    fn show_history_entry(&mut self, index: Option<usize>, history: &(impl History + ?Sized)) {
        if index == self.history_index {
            return;
        }
        if self.history_index.is_none() {
            self.draft = Some(self.buffer.clone());
        }

        self.history_index = index;
        self.buffer = match index.and_then(|index| history.get(index)) {
            Some(entry) => LineBuffer::new(entry),
            None => self.draft.take().unwrap_or_default(),
        };
    }

    fn start_search(&mut self) {
        self.search = Some(Search {
            query: String::new(),
            found: None,
            failed: false,
            original: self.buffer.clone(),
            original_index: self.history_index,
        });
    }

    /// Takes an action while searching, as readline's incremental search does:
    /// typing extends the query, Backspace shortens it, Ctrl-R finds an older
    /// match, Ctrl-G cancels the search, and Escape ends it. Anything else ends the
    /// search, and is then to take effect as usual.
    pub fn search_step(
        &mut self,
        action: &Action,
        history: &(impl History + ?Sized),
    ) -> SearchStep {
        let Some(mut search) = self.search.take() else {
            return SearchStep::Ended;
        };

        match action {
            Action::Insert(c) => {
                search.query.push(*c);
                // The current match may still match.
                let start = search.found.map_or(search.original_index, |i| Some(i + 1));
                self.search_from(&mut search, start, history);
            }
            Action::Function(InputFunction::BackwardDeleteChar) => {
                search.query.pop();
                let start = search.original_index;
                self.search_from(&mut search, start, history);
            }
            Action::Function(InputFunction::ReverseSearchHistory) => {
                let start = search.found.or(search.original_index);
                self.search_from(&mut search, start, history);
            }
            Action::Function(InputFunction::Abort) => {
                self.show_history_entry(search.original_index, history);
                self.buffer = search.original;
                return SearchStep::Taken;
            }
            Action::Unbound { escape: true } => return SearchStep::Taken,
            _ => return SearchStep::Ended,
        }

        self.search = Some(search);
        SearchStep::Taken
    }

    /// Searches for the query in history, back from (and not including) the given
    /// entry, or from the newest for `None`, and shows the most recent match, with
    /// the cursor at the start of what matched. If nothing matches, what was shown
    /// stays, as it does for an empty query, which matches nothing.
    fn search_from(
        &mut self,
        search: &mut Search,
        start: Option<usize>,
        history: &(impl History + ?Sized),
    ) {
        if search.query.is_empty() {
            search.found = None;
            search.failed = false;
            return;
        }

        let end = history.len();
        let end = start.map_or(end, |start| start.min(end));
        let found = (0..end).rev().find_map(|index| {
            let offset = history.get(index)?.find(&search.query)?;
            Some((index, offset))
        });

        search.failed = found.is_none();
        if let Some((index, offset)) = found {
            search.found = Some(index);
            self.show_history_entry(Some(index), history);
            self.buffer.move_to(offset);
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in tests")]
mod tests {
    use super::*;
    use InputFunction as F;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// An editor, what it's killed, and what it's drawn.
    struct Editing<'a> {
        editor: LineEditor<'a>,
        history: &'a [String],
        kills: KillBuffer,
        out: Vec<u8>,
    }

    impl<'a> Editing<'a> {
        fn new(history: &'a [String], style: Style) -> Self {
            Self {
                editor: LineEditor::new("$ ", String::new(), style),
                history,
                kills: KillBuffer::default(),
                out: vec![],
            }
        }

        /// Takes actions as the line reader does, returning the line if accepted.
        fn act(&mut self, actions: &[Action]) -> std::io::Result<Option<String>> {
            for action in actions {
                if self.editor.searching()
                    && self.editor.search_step(action, self.history) == SearchStep::Taken
                {
                    continue;
                }
                match action {
                    Action::Function(function) => {
                        let accepted = self.editor.perform(
                            function,
                            &mut self.kills,
                            self.history,
                            &mut self.out,
                            80,
                        )?;
                        if accepted.is_some() {
                            return Ok(accepted);
                        }
                    }
                    Action::Insert(c) => self.editor.insert(*c),
                    Action::Command(_) | Action::Unbound { .. } => {}
                }
                self.kills.chained = matches!(action, Action::Function(f) if is_kill(f));
            }
            Ok(None)
        }

        fn text(&self) -> &str {
            self.editor.buffer.text()
        }
    }

    fn f(function: F) -> Action {
        Action::Function(function)
    }

    fn typing(text: &str) -> Vec<Action> {
        text.chars().map(Action::Insert).collect()
    }

    #[test]
    fn performs_editing_functions() -> std::io::Result<()> {
        let mut e = Editing::new(&[], Style::Vt);
        e.editor.buffer = LineBuffer::new("one two three");

        e.act(&[f(F::BackwardWord), f(F::BackwardWord), f(F::BackwardChar)])?;
        assert_eq!(e.editor.buffer.cursor(), 3);
        e.act(&[f(F::ForwardChar), f(F::ForwardWord), f(F::ForwardWord)])?;
        assert_eq!(e.editor.buffer.cursor(), "one two three".len());
        e.act(&[f(F::BeginningOfLine), f(F::KillWord)])?;
        assert_eq!(e.text(), " two three");
        e.act(&[f(F::EndOfLine), f(F::BackwardKillWord)])?;
        assert_eq!(e.text(), " two ");
        e.act(&[f(F::BeginningOfLine), f(F::DeleteChar), f(F::KillLine)])?;
        assert_eq!(e.text(), "");
        e.act(&[f(F::Yank)])?;
        assert_eq!(e.text(), "two ");
        e.act(&[f(F::BackwardDeleteChar), f(F::UnixLineDiscard)])?;
        assert_eq!(e.text(), "");
        e.act(&[typing("ab"), vec![f(F::TransposeChars)]].concat())?;
        assert_eq!(e.text(), "ba");

        // Functions this editor doesn't perform do nothing.
        e.act(&[f(F::Undo), f(F::UpcaseWord)])?;
        assert_eq!(e.text(), "ba");
        assert_eq!(e.act(&[f(F::AcceptLine)])?, Some("ba".to_owned()));

        Ok(())
    }

    #[test]
    fn kills_and_yanks() -> std::io::Result<()> {
        let mut e = Editing::new(&[], Style::Vt);
        e.editor.buffer = LineBuffer::new("echo hello world");
        e.act(&[f(F::UnixWordRubout)])?;
        assert_eq!(e.text(), "echo hello ");
        e.act(&[f(F::BeginningOfLine), f(F::Yank)])?;
        assert_eq!(e.text(), "worldecho hello ");
        Ok(())
    }

    #[test]
    fn adds_consecutive_kills_together() -> std::io::Result<()> {
        let mut e = Editing::new(&[], Style::Vt);
        e.editor.buffer = LineBuffer::new("one two three four");

        // Backward kills go before what was killed, forward kills after.
        e.act(&[f(F::UnixWordRubout), f(F::UnixWordRubout)])?;
        assert_eq!(e.text(), "one two ");
        e.act(&[f(F::Yank)])?;
        assert_eq!(e.text(), "one two three four");
        e.act(&[f(F::BeginningOfLine), f(F::KillWord), f(F::KillWord)])?;
        assert_eq!(e.text(), " three four");
        e.act(&[f(F::EndOfLine), f(F::Yank)])?;
        assert_eq!(e.text(), " three fourone two");

        // Anything else in between starts afresh: only the last kill is yanked.
        e.act(&[
            f(F::BeginningOfLine),
            f(F::KillWord),
            f(F::ForwardChar),
            f(F::KillWord),
        ])?;
        assert_eq!(e.text(), "  two");
        e.act(&[f(F::Yank)])?;
        assert_eq!(e.text(), " fourone two");
        Ok(())
    }

    #[test]
    fn killing_nothing_keeps_what_was_killed() -> std::io::Result<()> {
        let mut e = Editing::new(&[], Style::Vt);
        e.editor.buffer = LineBuffer::new("echo word");
        e.act(&[f(F::UnixWordRubout)])?;
        // At the end of the line, there's nothing to kill.
        e.act(&[f(F::ForwardChar), f(F::KillLine), f(F::Yank)])?;
        assert_eq!(e.text(), "echo word");
        Ok(())
    }

    #[test]
    fn browses_history_and_keeps_the_draft() -> std::io::Result<()> {
        let history = strings(&["first", "second"]);
        let mut e = Editing::new(&history, Style::Vt);

        e.act(&typing("dra"))?;
        e.act(&[f(F::PreviousHistory)])?;
        assert_eq!(e.text(), "second");
        e.act(&[f(F::PreviousHistory), f(F::PreviousHistory)])?;
        assert_eq!(e.text(), "first");
        assert_eq!(e.out, b"\x07");
        e.act(&[f(F::NextHistory), f(F::NextHistory)])?;
        assert_eq!(e.text(), "dra");
        Ok(())
    }

    #[test]
    fn jumps_to_either_end_of_history() -> std::io::Result<()> {
        let history = strings(&["first", "second"]);
        let mut e = Editing::new(&history, Style::Vt);

        e.act(&typing("x"))?;
        e.act(&[f(F::BeginningOfHistory)])?;
        assert_eq!(e.text(), "first");
        e.act(&[f(F::NextHistory)])?;
        assert_eq!(e.text(), "second");
        e.act(&[f(F::EndOfHistory)])?;
        assert_eq!(e.text(), "x");

        // Past the end, there's nothing more to show.
        e.act(&[f(F::NextHistory)])?;
        assert_eq!(e.text(), "x");
        assert_eq!(e.out, b"\x07");
        Ok(())
    }

    #[test]
    fn keeps_the_draft_when_already_at_the_end() -> std::io::Result<()> {
        let mut e = Editing::new(&[], Style::Vt);
        e.act(&typing("foo"))?;
        e.act(&[f(F::EndOfHistory), f(F::BeginningOfHistory)])?;
        assert_eq!(e.text(), "foo");
        Ok(())
    }

    #[test]
    fn keeps_the_draft_through_a_search() -> std::io::Result<()> {
        let history = strings(&["one", "two"]);
        let mut e = Editing::new(&history, Style::Vt);
        e.act(&typing("foo"))?;
        e.act(&[vec![f(F::ReverseSearchHistory)], typing("tw")].concat())?;
        assert_eq!(e.text(), "two");
        // Moving on ends the search; going back to the end shows the draft.
        e.act(&[f(F::ForwardChar), f(F::NextHistory)])?;
        assert_eq!(e.text(), "foo");
        Ok(())
    }

    #[test]
    fn rings_the_bell_only_in_the_vt_style() -> std::io::Result<()> {
        let mut e = Editing::new(&[], Style::Basic);
        e.act(&[f(F::PreviousHistory), f(F::NextHistory), f(F::Abort)])?;
        assert_eq!(e.out, b"");

        let mut e = Editing::new(&[], Style::Vt);
        e.act(&[f(F::Abort)])?;
        assert_eq!(e.out, b"\x07");
        Ok(())
    }

    #[test]
    fn searches_history_incrementally() -> std::io::Result<()> {
        let history = strings(&["echo one", "ls -l", "echo two"]);
        let mut e = Editing::new(&history, Style::Vt);

        e.act(&[vec![f(F::ReverseSearchHistory)], typing("echo")].concat())?;
        assert_eq!(e.text(), "echo two");
        e.act(&[f(F::ReverseSearchHistory)])?;
        assert_eq!(e.text(), "echo one");
        assert_eq!(
            e.editor.search_prompt().as_deref(),
            Some("(reverse-i-search)`echo': ")
        );

        // Another key ends the search, keeping the match, and takes effect.
        e.act(&[f(F::EndOfLine)])?;
        assert!(!e.editor.searching());
        e.act(&typing("!"))?;
        assert_eq!(e.text(), "echo one!");

        // Like readline, a search goes back from the entry being shown.
        e.act(&[f(F::ReverseSearchHistory), Action::Insert('l')])?;
        assert_eq!(
            e.editor.search_prompt().as_deref(),
            Some("(failed reverse-i-search)`l': ")
        );

        // Cancelling restores the line as it was.
        e.act(&[f(F::Abort)])?;
        assert_eq!(e.text(), "echo one!");
        Ok(())
    }

    #[test]
    fn refines_ends_and_cancels_searches() -> std::io::Result<()> {
        let history = strings(&["make test", "cargo build", "make all"]);
        let mut e = Editing::new(&history, Style::Vt);

        e.act(&[Action::Insert('d'), f(F::ReverseSearchHistory)])?;
        assert_eq!(
            e.editor.search_prompt().as_deref(),
            Some("(reverse-i-search)`': ")
        );
        e.act(&typing("ma"))?;
        assert_eq!(e.text(), "make all");
        // Searching again finds an older match, with the cursor on the match.
        e.act(&[f(F::ReverseSearchHistory)])?;
        assert_eq!(e.text(), "make test");
        assert_eq!(e.editor.buffer.cursor(), 0);
        // With no older match, the last one stays.
        e.act(&[f(F::ReverseSearchHistory)])?;
        assert_eq!(e.text(), "make test");

        // A query that matches nothing fails, until it's shortened.
        e.act(&typing("z"))?;
        assert_eq!(
            e.editor.search_prompt().as_deref(),
            Some("(failed reverse-i-search)`maz': ")
        );
        e.act(&[vec![f(F::BackwardDeleteChar); 3], typing("b")].concat())?;
        assert_eq!(e.text(), "cargo build");
        assert_eq!(e.editor.buffer.cursor(), "cargo ".len());

        // Escape ends the search, keeping the match, as in readline.
        e.act(&[Action::Unbound { escape: true }])?;
        assert!(!e.editor.searching());
        assert_eq!(e.text(), "cargo build");

        // Ctrl-G cancels, going back to the line as it was before searching. (This
        // search goes back from the entry shown, so it finds the older `make`.)
        e.act(&[vec![f(F::ReverseSearchHistory)], typing("make")].concat())?;
        assert_eq!(e.text(), "make test");
        e.act(&[f(F::Abort)])?;
        assert!(!e.editor.searching());
        assert_eq!(e.text(), "cargo build");
        Ok(())
    }

    #[test]
    fn keeps_the_last_match_when_a_search_fails() -> std::io::Result<()> {
        let history = strings(&["cargo test", "make build", "cargo doc"]);
        let mut e = Editing::new(&history, Style::Vt);

        e.act(&[vec![f(F::ReverseSearchHistory)], typing("ca")].concat())?;
        e.act(&[f(F::ReverseSearchHistory), f(F::ReverseSearchHistory)])?;
        assert_eq!(e.text(), "cargo test");
        assert_eq!(
            e.editor.search_prompt().as_deref(),
            Some("(failed reverse-i-search)`ca': ")
        );

        // The search goes on from the match that's shown.
        e.act(&typing("r"))?;
        assert_eq!(e.text(), "cargo test");
        assert_eq!(
            e.editor.search_prompt().as_deref(),
            Some("(reverse-i-search)`car': ")
        );
        Ok(())
    }

    #[test]
    fn leaves_the_line_as_it_is_for_an_empty_search() -> std::io::Result<()> {
        let history = strings(&["echo aaa", "echo bbb"]);
        let mut e = Editing::new(&history, Style::Vt);
        e.act(&typing("echo DRAFT"))?;

        // An empty query matches nothing, so the line stays.
        e.act(&[f(F::ReverseSearchHistory), f(F::BackwardDeleteChar)])?;
        assert_eq!(e.text(), "echo DRAFT");
        e.act(&[f(F::ReverseSearchHistory)])?;
        assert_eq!(e.text(), "echo DRAFT");
        assert_eq!(
            e.editor.search_prompt().as_deref(),
            Some("(reverse-i-search)`': ")
        );

        // Nor does shortening the query to nothing go elsewhere.
        e.act(&[Action::Insert('a'), f(F::BackwardDeleteChar)])?;
        assert_eq!(e.text(), "echo aaa");
        Ok(())
    }

    #[test]
    fn clears_the_screen() -> std::io::Result<()> {
        let no_history: &[String] = &[];
        let mut editor = LineEditor::new("$ ", "above".to_owned(), Style::Vt);
        let mut out = vec![];
        editor.perform(
            &F::ClearScreen,
            &mut KillBuffer::default(),
            no_history,
            &mut out,
            80,
        )?;
        assert_eq!(out, b"\x1b[H\x1b[2Jabove\n");

        // Without cursor addressing, the line is drawn afresh below instead.
        let mut editor = LineEditor::new("$ ", "above".to_owned(), Style::Basic);
        editor.insert('x');
        let mut out = vec![];
        editor.perform(
            &F::ClearScreen,
            &mut KillBuffer::default(),
            no_history,
            &mut out,
            80,
        )?;
        assert_eq!(out, b"$ x\nabove\n");
        Ok(())
    }
}
