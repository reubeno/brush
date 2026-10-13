//! Builtin help, as data the shell renders.

/// A builtin's help.
///
/// It is data: plain text, which the shell renders in every form
/// ([`HelpForm`]), keeping `help -s`, `help -d`, `help name`, `name --help`,
/// and the usage line of a usage error consistent with each other. Its details
/// are wrapped to [`Help::DETAILS_WIDTH`], so plain help is the same wherever
/// it's printed. A builtin can also render its help for a terminal
/// ([`ParseArgs::terminal_help`](super::ParseArgs::terminal_help)): the same
/// text, wrapped to the terminal's width and painted for it. The shell decides
/// when to use it (see [`crate::ExecutionContext::help_style`]). Non-exhaustive so
/// it can carry more later; construct it with [`Help::new`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Help {
    /// The synopsis, including the builtin's name but no `name: ` prefix, e.g.
    /// `cd [-L|[-P [-e]] [-@]] [dir]`.
    pub synopsis: String,
    /// A one-line description, e.g. `Change the shell working directory.`
    pub description: String,
    /// The rest of the full help, as text, wrapped to
    /// [`DETAILS_WIDTH`](Self::DETAILS_WIDTH) columns in plain help; may be
    /// empty. Each of its lines is rendered indented, after the synopsis and
    /// description.
    pub details: String,
}

/// How far [`Help::render`] indents the lines that follow the synopsis.
pub(crate) const DETAILS_INDENT: usize = 4;

impl Help {
    /// The width, in columns, that plain help's details are wrapped to: the
    /// full form, indented as [`render`](Self::render) indents it, fits in 80
    /// columns.
    pub const DETAILS_WIDTH: usize = 80 - DETAILS_INDENT;

    /// Returns help with the given synopsis and description, and no details.
    ///
    /// # Arguments
    ///
    /// * `synopsis` - The synopsis, including the builtin's name.
    /// * `description` - A one-line description.
    pub fn new(synopsis: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            synopsis: synopsis.into(),
            description: description.into(),
            details: String::new(),
        }
    }

    /// Adds the rest of the full help.
    ///
    /// # Arguments
    ///
    /// * `details` - The text that follows the description.
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = details.into();
        self
    }

    /// Renders one form of the help for the builtin invoked as `name`, as bash
    /// renders it, ending with a newline.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under.
    /// * `form` - Which form to render.
    pub fn render(&self, name: &str, form: HelpForm) -> String {
        match form {
            HelpForm::Synopsis => format!("{name}: {}\n", self.synopsis),
            HelpForm::Description => format!("{name} - {}\n", self.description),
            HelpForm::Full => {
                let mut text = format!("{name}: {}\n    {}\n", self.synopsis, self.description);
                if !self.details.is_empty() {
                    text.push_str("    \n");
                    for line in self.details.lines() {
                        text.push_str("    ");
                        text.push_str(line);
                        text.push('\n');
                    }
                }
                text
            }
        }
    }
}

/// A form of a builtin's help; see [`Help::render`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HelpForm {
    /// `name: synopsis`, as `help -s` prints.
    Synopsis,
    /// `name - description`, as `help -d` prints.
    Description,
    /// The synopsis, description, and details, as `help name` and `name --help`
    /// print.
    Full,
}

/// How help printed to a terminal (or to output that color is forced on for)
/// is rendered, as the shell decides it; see
/// [`ParseArgs::terminal_help`](super::ParseArgs::terminal_help).
///
/// Non-exhaustive so it can carry more later (a color depth, say); construct
/// it with [`TerminalStyle::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TerminalStyle {
    /// How many columns the details may take: the output's width, less the
    /// indent [`Help::render`] adds.
    pub width: usize,
    /// Whether the help is painted with color.
    pub color: bool,
}

impl TerminalStyle {
    /// Returns a style for details `width` columns wide, painted with color
    /// if `color`.
    ///
    /// # Arguments
    ///
    /// * `width` - How many columns the details may take.
    /// * `color` - Whether the help is painted with color.
    pub const fn new(width: usize, color: bool) -> Self {
        Self { width, color }
    }

    /// Returns the style for output `columns` wide: the details may take
    /// what's left after the indent [`Help::render`] adds.
    pub(crate) const fn for_columns(columns: usize, color: bool) -> Self {
        Self::new(columns.saturating_sub(DETAILS_INDENT), color)
    }
}

/// Returns whether help printed to output that is (or isn't) a terminal is
/// painted with color, from the variables `var` gives, by the conventions
/// [`help_style`](crate::ExecutionContext::help_style) lists.
///
/// # Arguments
///
/// * `is_terminal` - Whether the output is a terminal.
/// * `var` - Returns a variable's value, if it's set.
pub(crate) fn wants_color(is_terminal: bool, var: impl Fn(&str) -> Option<String>) -> bool {
    if var("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    match var("FORCE_COLOR").as_deref() {
        Some("false" | "0") => return false,
        Some(_) => return true,
        None => {}
    }
    if var("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty() && v != "0") {
        return true;
    }
    let refused = var("CLICOLOR").is_some_and(|v| v == "0");
    let dumb = var("TERM").is_none_or(|term| term == "dumb");
    is_terminal && !refused && !dumb
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(is_terminal: bool, vars: &[(&str, &str)]) -> bool {
        wants_color(is_terminal, |name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_owned())
        })
    }

    #[test]
    fn a_terminal_s_details_take_the_columns_the_indent_leaves() {
        assert_eq!(TerminalStyle::for_columns(100, true).width, 96);
        assert_eq!(TerminalStyle::for_columns(2, false).width, 0);
    }

    #[test]
    fn color_follows_the_usual_conventions() {
        let xterm = ("TERM", "xterm");
        assert!(color(true, &[xterm]));
        assert!(!color(false, &[xterm]));
        assert!(!color(true, &[]));
        assert!(!color(true, &[("TERM", "dumb")]));
        assert!(!color(true, &[xterm, ("NO_COLOR", "1")]));
        assert!(!color(true, &[xterm, ("CLICOLOR", "0")]));
        assert!(color(false, &[("FORCE_COLOR", "2")]));
        assert!(color(false, &[("FORCE_COLOR", "")]));
        assert!(color(false, &[("CLICOLOR_FORCE", "1")]));
        assert!(!color(true, &[xterm, ("FORCE_COLOR", "0")]));
        assert!(!color(false, &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")]));
    }
}
