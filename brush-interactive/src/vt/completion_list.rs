//! Completing the word at the cursor, and listing the candidates.

use std::io::Write;

use super::line_editor::LineEditor;
use super::render::display_width;
use super::style::Style;
use crate::{ShellError, completion};

/// Completes the word at the cursor, as readline's `complete` does: a single
/// candidate replaces the word, and several are handled as `style` has them be.
///
/// # Arguments
///
/// * `shell` - The shell, which generates the candidates.
/// * `editor` - The line being edited.
/// * `out` - The terminal.
/// * `columns` - The terminal's width.
/// * `style` - How the backend draws.
pub(crate) async fn complete(
    shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
    editor: &mut LineEditor<'_>,
    out: &mut (impl Write + Send),
    columns: usize,
    style: Style,
) -> Result<(), ShellError> {
    let completions = {
        let mut shell = shell.lock().await;
        completion::complete_async(&mut shell, editor.buffer.text(), editor.buffer.cursor()).await
    };

    let text = editor.buffer.text();
    let range = completions.insertion_index..completions.insertion_index + completions.delete_count;
    let Some(word) = text.get(range.clone()) else {
        return Ok(());
    };

    let names = |candidates: &[String]| -> Vec<String> {
        candidates
            .iter()
            .map(|c| display_name(c, &completions.options))
            .collect()
    };

    match (completions.candidates.as_slice(), style) {
        ([], _) => editor.bell(out)?,
        ([candidate], _) => editor.buffer.replace(range, candidate),
        // As readline does by default: extend the word as far as the candidates
        // agree; if it can't, ring the bell, and list them on a second try.
        (candidates, Style::Vt) => {
            let common = common_prefix(candidates);
            // Don't insert a prefix ending in a lone escape (e.g., `a\` from `a\ b`
            // and `a\'b`), which would change how the rest of the line parses.
            let dangling = (common.len() - common.trim_end_matches('\\').len()) % 2 == 1;
            if common.len() > word.len() && !dangling {
                editor.buffer.replace(range, &common);
            } else if editor.completed_last {
                editor.finish(out, columns)?;
                out.write_all(format_in_columns(&names(candidates), columns).as_bytes())?;
                editor.show_prompt_above(out)?;
            } else {
                editor.bell(out)?;
            }
        }
        // List them straight away, one per line.
        (candidates, Style::Basic) => {
            editor.finish(out, columns)?;
            for name in names(candidates) {
                out.write_all(name.as_bytes())?;
                out.write_all(b"\n")?;
            }
            editor.show_prompt_above(out)?;
        }
    }

    Ok(())
}

/// Returns the longest prefix that all of `candidates` share.
fn common_prefix(candidates: &[String]) -> String {
    let Some((first, rest)) = candidates.split_first() else {
        return String::new();
    };

    let mut len = first.len();
    for candidate in rest {
        len = first
            .char_indices()
            .zip(candidate.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8())
            .min(len);
    }

    first.get(..len).unwrap_or_default().to_owned()
}

/// Returns how a completion candidate is shown in a list of candidates, as bash
/// shows it: file names unquoted and without their directory, and without the
/// space that would follow them.
fn display_name(candidate: &str, options: &brush_core::completion::ProcessingOptions) -> String {
    let mut word = candidate;
    // Drop the space completion puts after a word, but not an escaped space that
    // ends the word itself (e.g., in `foo\ `, for a file named `foo `).
    if let Some(trimmed) = word.strip_suffix(' ') {
        let backslashes = trimmed.len() - trimmed.trim_end_matches('\\').len();
        if backslashes % 2 == 0 {
            word = trimmed;
        }
    }
    if !options.treat_as_filenames {
        return word.to_owned();
    }

    // File names were quoted for the command line; list them as they're named.
    let name = if options.no_autoquote_filenames {
        word.to_owned()
    } else {
        brush_parser::unquote_str(word)
    };
    let trimmed = brush_core::sys::fs::strip_path_separator_suffix(&name);
    match brush_core::sys::fs::rfind_path_separator(trimmed) {
        Some(index) => name.get(index + 1..).unwrap_or(&name).to_owned(),
        None => name,
    }
}

/// Lays out `names` in columns, ordered down then across, as `ls` does.
fn format_in_columns(names: &[String], columns: usize) -> String {
    let width = names.iter().map(|n| display_width(n)).max().unwrap_or(0) + 2;
    let per_row = (columns / width).max(1);
    let rows = names.len().div_ceil(per_row);

    let mut output = String::new();
    for row in 0..rows {
        let mut line = String::new();
        for name in names.iter().skip(row).step_by(rows) {
            line.push_str(name);
            line.push_str(&" ".repeat(width - display_width(name)));
        }
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn finds_common_prefixes() {
        assert_eq!(common_prefix(&strings(&["foo", "foobar", "fox"])), "fo");
        assert_eq!(common_prefix(&strings(&["été", "étage"])), "ét");
        assert_eq!(common_prefix(&strings(&["a", "b"])), "");
        assert_eq!(common_prefix(&strings(&["same", "same"])), "same");
        assert_eq!(common_prefix(&[]), "");
    }

    #[test]
    fn lays_out_candidates_in_columns() {
        let names = strings(&["a", "bb", "ccc", "d", "e"]);
        assert_eq!(format_in_columns(&names, 12), "a    d\nbb   e\nccc\n");
        assert_eq!(format_in_columns(&names, 4), "a\nbb\nccc\nd\ne\n");
    }

    #[test]
    fn lays_out_names_wider_than_the_terminal_one_per_row() {
        let names = strings(&["abcdef", "ghijkl"]);
        assert_eq!(format_in_columns(&names, 4), "abcdef\nghijkl\n");
    }

    #[test]
    fn lays_out_wide_names_by_the_cells_they_take() {
        let names = strings(&["日本", "ab", "c"]);
        assert_eq!(format_in_columns(&names, 12), "日本  c\nab\n");
    }

    #[test]
    fn shows_file_names_without_their_directories() {
        let filenames = brush_core::completion::ProcessingOptions::default();
        assert_eq!(display_name("dir/sub/file ", &filenames), "file");
        assert_eq!(display_name("dir/sub/", &filenames), "sub/");
        assert_eq!(display_name("plain ", &filenames), "plain");
        assert_eq!(display_name("dossier/été ", &filenames), "été");
        assert_eq!(display_name("dir/my\\ file ", &filenames), "my file");
        assert_eq!(display_name("dir/my\\ sub/", &filenames), "my sub/");
        assert_eq!(display_name("'dir/my file' ", &filenames), "my file");
        assert_eq!(display_name("dir/foo\\  ", &filenames), "foo ");
        assert_eq!(display_name("dir/foo\\ ", &filenames), "foo ");
        assert_eq!(display_name("dir/foo\\\\ ", &filenames), "foo\\");

        let unquoted = brush_core::completion::ProcessingOptions {
            no_autoquote_filenames: true,
            ..Default::default()
        };
        assert_eq!(display_name("dir/it's ", &unquoted), "it's");

        let words = brush_core::completion::ProcessingOptions {
            treat_as_filenames: false,
            ..Default::default()
        };
        assert_eq!(display_name("a/b ", &words), "a/b");
        assert_eq!(display_name("a\\ b ", &words), "a\\ b");
    }
}
