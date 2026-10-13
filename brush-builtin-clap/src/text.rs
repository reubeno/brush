//! Text helpers for rendering a builtin's help.

/// Returns the paragraphs of a builtin's description after its first (the
/// one-line description), wrapped to `width` columns, for the start of its
/// help's details; empty if there are none.
///
/// A paragraph of prose is rewrapped. One that's laid out by hand (a list, or
/// an indented block) is kept as it is.
///
/// # Arguments
///
/// * `description` - The whole description, its paragraphs separated by a
///   blank line.
/// * `width` - The columns to wrap to.
pub(crate) fn rest_of_description(description: &str, width: usize) -> String {
    let mut paragraphs = description.trim().split("\n\n").map(str::trim_end);
    let _first = paragraphs.next();
    paragraphs
        .filter(|paragraph| !paragraph.trim().is_empty())
        .map(|paragraph| {
            if paragraph.lines().any(laid_out) {
                paragraph.to_owned()
            } else {
                wrap(paragraph, width)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Whether a line is laid out by hand: indented, or a list item.
fn laid_out(line: &str) -> bool {
    line.starts_with([' ', '\t'])
        || line.starts_with("- ")
        || line.starts_with("* ")
        || line
            .split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Greedily wraps prose to `width` columns, at spaces.
fn wrap(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line_len = 0;
    for word in text.split_whitespace() {
        let len = word.chars().count();
        if line_len > 0 && line_len + 1 + len > width {
            out.push('\n');
            line_len = 0;
        } else if line_len > 0 {
            out.push(' ');
            line_len += 1;
        }
        out.push_str(word);
        line_len += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rest_of_a_description_is_wrapped_unless_laid_out() {
        let description = "Do a thing.\n\nThe thing is done\nwell, and done fast.\n\n- one\n- two";
        assert_eq!(
            rest_of_description(description, 20),
            "The thing is done\nwell, and done fast.\n\n- one\n- two"
        );
        assert_eq!(rest_of_description("Do a thing.", 20), "");
        assert_eq!(rest_of_description("", 20), "");
    }
}
