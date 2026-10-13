//! How a [`super::VtInputBackend`] draws and responds to Tab.

/// How a [`super::VtInputBackend`] shows the line being edited and responds to
/// Tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Style {
    /// For people at a terminal that understands VT100-style escape sequences.
    /// Each change redraws the line with them, which handles lines that wrap
    /// across rows. As in readline, Tab extends an ambiguous word as far as the
    /// candidates agree; if it can't, it rings the bell, and a second Tab lists
    /// the candidates in columns.
    #[default]
    Vt,
    /// For programs that drive the shell and read what it prints, such as test
    /// harnesses. The line is drawn with backspaces alone, rewriting just what
    /// changed, so a line that wraps across rows isn't redrawn correctly; Tab
    /// lists ambiguous completions straight away, one per line; and the bell
    /// never rings.
    Basic,
}
