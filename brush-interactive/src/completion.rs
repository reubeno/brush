use brush_core::escape;

#[allow(dead_code)]
pub(crate) async fn complete_async(
    shell: &mut brush_core::Shell<impl brush_core::ShellExtensions>,
    line: &str,
    pos: usize,
) -> brush_core::completion::Completions {
    // Wait for the completions to come back or interruption, whichever happens first.
    // Intentionally ignore any errors that arise.
    let result = {
        let completion_future = shell.complete(line, pos);
        tokio::pin!(completion_future);

        tokio::select! {
            result = &mut completion_future => {
                result
            }
            _ = tokio::signal::ctrl_c() => {
                Err(brush_core::ErrorKind::Interrupted.into())
            },
        }
    };

    let mut completions = result.unwrap_or_else(|_| brush_core::completion::Completions {
        insertion_index: pos,
        delete_count: 0,
        candidates: Vec::new(),
        options: brush_core::completion::ProcessingOptions::default(),
    });

    // Look at the line up to 'pos' to check if we're in an unterminated
    // single or double quote string.
    let mut quote_char: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if i >= pos {
            break;
        }

        if escaped {
            escaped = false;
            continue;
        }

        if let Some(q) = quote_char {
            if c == q {
                quote_char = None;
            }
        } else if c == '\\' {
            escaped = true;
        } else if c == '\'' || c == '\"' {
            quote_char = Some(c);
        }
    }

    let completing_end_of_line = pos == line.len();
    let working_dir = shell.working_dir();

    // Deduplicate the candidates (retaining order), then postprocess them.
    completions.candidates = completions
        .candidates
        .into_iter()
        .collect::<indexmap::IndexSet<_>>()
        .into_iter()
        .map(|candidate| {
            postprocess_completion_candidate(
                candidate,
                &completions.options,
                working_dir,
                completing_end_of_line,
                quote_char,
            )
        })
        .collect();

    completions
}

#[allow(dead_code)]
fn postprocess_completion_candidate(
    mut candidate: String,
    options: &brush_core::completion::ProcessingOptions,
    working_dir: &brush_core::ResolvedPath,
    completing_end_of_line: bool,
    quote_char: Option<char>,
) -> String {
    if options.treat_as_filenames {
        // Check if it's a directory.
        if !brush_core::sys::fs::ends_with_path_separator(&candidate) {
            if working_dir.join(&candidate).is_dir() {
                // Use forward slash: backslash is the shell escape character.
                candidate.push('/');
            }
        }

        if !options.no_autoquote_filenames {
            let quote_mode = match quote_char {
                Some('\'') => escape::QuoteMode::SingleQuote,
                Some('\"') => escape::QuoteMode::DoubleQuote,
                _ => escape::QuoteMode::BackslashEscape,
            };

            // Like bash, leave a `~` unquoted, so a `~user` completion still expands.
            let options = escape::QuoteOptions::builder()
                .preferred_mode(quote_mode)
                .leave_tilde(true)
                .build();
            candidate = escape::quote(&candidate, &options).to_string();
        }
    }
    if completing_end_of_line && !options.no_trailing_space_at_end_of_line {
        if !options.treat_as_filenames || !brush_core::sys::fs::ends_with_path_separator(&candidate)
        {
            candidate.push(' ');
        }
    }

    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quotes a file-name candidate completed at the end of the line.
    fn quoted_file_name(candidate: &str) -> String {
        let options = brush_core::completion::ProcessingOptions {
            treat_as_filenames: true,
            ..Default::default()
        };
        postprocess_completion_candidate(
            candidate.to_owned(),
            &options,
            &brush_core::ResolvedPath::default(),
            true,
            None,
        )
    }

    #[test]
    fn file_names_are_quoted_like_bash() {
        // Like bash, a `~user` candidate isn't quoted, so it still expands...
        assert_eq!(quoted_file_name("~root"), "~root ");
        // ...but a leading `#` is, so it doesn't start a comment.
        assert_eq!(quoted_file_name("#hash"), r"\#hash ");
        assert_eq!(quoted_file_name("a b"), r"a\ b ");
        // Like bash, a comma isn't special on its own.
        assert_eq!(quoted_file_name("a,b"), "a,b ");
    }
}
