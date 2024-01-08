#[derive(
    rustyline::Helper,
    rustyline::Completer,
    rustyline::Hinter,
    rustyline::Validator,
    rustyline::Highlighter,
)]
pub(crate) struct EditorHelper {
    // #[rustyline(Completer)]
    // completer: rustyline::completion::FilenameCompleter,
    #[rustyline(Hinter)]
    hinter: rustyline::hint::HistoryHinter,
}

impl EditorHelper {
    pub(crate) fn new(_shell: &shell::Shell) -> Self {
        // let completer = rustyline::completion::FilenameCompleter::new();
        let hinter = rustyline::hint::HistoryHinter::new();

        Self { hinter }
    }
}
