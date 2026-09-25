//! Generates the file names that complete a word, and resolves what file-name candidates
//! name.

use std::{borrow::Cow, collections::HashMap, path::Path};

use brush_parser::unquote_str;

use super::{
    CandidateKind, Context, ResolvedCandidate,
    quoting::{self, WordQuoting},
    shell_pattern,
};
use crate::{Shell, escape, expansion, extensions, patterns, sys};

/// Which file names [`file_completions`] generates.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum FileKinds {
    /// All file names.
    All,
    /// Only directory names.
    DirsOnly,
}

/// Generates the file names of the given kinds that complete the context's word. Like bash,
/// they show its directory part as typed (e.g. `~/`), unless the `direxpand` option says
/// to show its parameters expanded.
pub(super) async fn file_completions(
    shell: &Shell<impl extensions::ShellExtensions>,
    context: &Context<'_>,
    kinds: FileKinds,
) -> Vec<String> {
    let FileNamePrefix {
        shown_dir,
        expanded_dir,
        file_name,
    } = file_name_prefix(shell, context).await;
    let prefix = std::format!("{expanded_dir}{file_name}");
    let dot_dirs: &[&str] = match prefix.as_str() {
        "." => &[".", ".."],
        ".." => &[".."],
        _ => &[],
    };

    let path_filter = |path: &Path| kinds == FileKinds::All || shell.absolute_path(path).is_dir();
    let pattern = shell_pattern(
        shell,
        vec![
            patterns::PatternPiece::Literal(prefix),
            patterns::PatternPiece::Pattern("*".to_owned()),
        ],
    );

    let mut completions: Vec<String> = pattern
        .expand(
            shell.working_dir(),
            Some(&path_filter),
            &patterns::FilenameExpansionOptions::default(),
        )
        .unwrap_or_default()
        .into_paths()
        .into_iter()
        .map(|p| match sys::fs::normalize_path_separators(&p) {
            Cow::Borrowed(_) => p,
            Cow::Owned(normalized) => normalized,
        })
        .map(|path| {
            if let Some(rest) = path.strip_prefix(expanded_dir.as_str()) {
                std::format!("{shown_dir}{rest}")
            } else {
                path
            }
        })
        .chain(dot_dirs.iter().map(|&dir| dir.to_owned()))
        .collect();

    completions.sort();
    completions.dedup();
    completions
}

/// The start of the file names that complete a word.
struct FileNamePrefix {
    /// Their directory part as candidates show it.
    shown_dir: String,
    /// Their directory part, expanded to search for them.
    expanded_dir: String,
    /// The start of the file name in that directory.
    file_name: String,
}

/// Returns the start of the file names completing the context's word, dequoted as the
/// module docs describe. Its directory part is expanded to search (e.g. `~/` or `$HOME/`),
/// but the rest, the file name being completed, isn't: e.g. a `$` in it is literal.
async fn file_name_prefix(
    shell: &Shell<impl extensions::ShellExtensions>,
    context: &Context<'_>,
) -> FileNamePrefix {
    let WordQuoting {
        quote,
        dequote,
        dequote_again,
    } = context.quoting;

    let mut word = Cow::Borrowed(context.word);
    if dequote_again {
        word = quoting::unquote_in_quote(&word, quote).into();
    }

    // Complete the word as if it followed the unclosed quote it's in.
    if let Some(q) = quote {
        word = std::format!("{}{word}", q.as_char()).into();
    }

    let (dir, file_name) = split_dir(&word);

    let expanded_dir = if dequote || !dir.contains(['$', '`']) {
        expand_directory(shell, &unquote_str(dir), DirExpansion::Dequoted).await
    } else {
        expand_directory(shell, dir, DirExpansion::AsTyped).await
    };

    // Like bash, candidates show the directory as typed (dequoted along with the word), or
    // with `direxpand`, with its parameters (but not a leading `~`) expanded.
    let shown_dir = if shell.options().expand_dir_names_on_completion {
        expand_directory(shell, &unquote_str(dir), DirExpansion::ParametersOnly).await
    } else if dequote {
        unquote_str(dir)
    } else {
        dir.to_owned()
    };

    let file_name = if dequote {
        quoting::unquote_in_quote(file_name, quoting::open_quote(dir))
    } else {
        file_name.to_owned()
    };

    FileNamePrefix {
        shown_dir,
        expanded_dir,
        file_name,
    }
}

/// Splits `path` into its directory part, through its last `/`, and the rest.
fn split_dir(path: &str) -> (&str, &str) {
    // TODO(windows): split on `\` too, once it can be told apart from quoting.
    let dir_len = path.rfind('/').map_or(0, |index| index + 1);
    path.split_at(dir_len)
}

/// Splits `path` into its leading `~user/` (or `~user`, if that's all it is), if it has
/// one, and the rest.
pub(super) fn split_tilde_prefix(path: &str) -> (&str, &str) {
    let tilde_len = if path.starts_with('~') {
        path.find('/').map_or(path.len(), |index| index + 1)
    } else {
        0
    };
    path.split_at(tilde_len)
}

/// How [`expand_directory`] expands a directory's text.
#[derive(Clone, Copy)]
enum DirExpansion {
    /// The text is dequoted, so its backslashes are literal. A leading `~` and parameters
    /// expand.
    Dequoted,
    /// The text is as typed: its backslashes escape as they do in double quotes (e.g. `\$`).
    /// A leading `~` and parameters expand.
    AsTyped,
    /// The text is dequoted, and just its parameters expand, not a leading `~`: as
    /// `direxpand` shows it.
    ParametersOnly,
}

/// Expands `dir`, the directory part of a word being completed (e.g. `~/` or `$HOME/`), as
/// `how` says. Like bash, it's left as is if a directory by that name exists, and nothing
/// but a leading `~` and parameters expand -- even ones that were quoted (e.g. in
/// `'$HOME/`): e.g. a `*` in it is literal.
async fn expand_directory(
    shell: &Shell<impl extensions::ShellExtensions>,
    dir: &str,
    how: DirExpansion,
) -> String {
    if shell.absolute_path(Path::new(dir)).is_dir() {
        return dir.to_owned();
    }

    // Expand a word that leaves just a leading `~user/` unquoted and double-quotes the
    // rest, keeping only the chars live there that should be.
    let (expand_tilde, live): (bool, &[char]) = match how {
        DirExpansion::Dequoted => (true, &['$']),
        DirExpansion::AsTyped => (true, &['$', '\\']),
        DirExpansion::ParametersOnly => (false, &['$']),
    };
    let (tilde, rest) = if expand_tilde {
        split_tilde_prefix(dir)
    } else {
        ("", dir)
    };
    let word = std::format!("{tilde}{}", escape::double_quote_leaving(rest, live));

    // Expand in a copy of the shell, so completing can't change the shell: expansion has
    // side effects in several places (e.g. `${x:=y}`, `$((x++))`, or `$RANDOM`). This only
    // runs for a directory that doesn't exist as named (e.g. `~/`), so the copy is rare.
    let mut throwaway_shell = shell.clone();
    let params = throwaway_shell.default_exec_params();
    let options = expansion::ExpanderOptions {
        execute_command_substitutions: false,
        ..Default::default()
    };
    expansion::basic_expand_word_with_options(&mut throwaway_shell, &params, &word, &options)
        .await
        .unwrap_or_else(|_err| dir.to_owned())
}

/// Resolves candidates that are file names: what each names, and whether that's a
/// directory (which one ending with a `/` is taken to be).
pub(super) async fn resolve_file_names(
    shell: &Shell<impl extensions::ShellExtensions>,
    texts: Vec<String>,
) -> Vec<ResolvedCandidate> {
    // Candidates mostly share directories, so expand each just once.
    let mut expanded_dirs = HashMap::new();

    let mut candidates = Vec::with_capacity(texts.len());
    for text in texts {
        let expanded = expand_file_name(shell, &text, &mut expanded_dirs).await;
        let is_dir = sys::fs::ends_with_path_separator(&text)
            || shell
                .absolute_path(Path::new(expanded.as_deref().unwrap_or(&text)))
                .is_dir();
        candidates.push(ResolvedCandidate {
            text,
            kind: CandidateKind::FileName { is_dir },
            expanded,
        });
    }
    candidates
}

/// If `text`, an unquoted file name, names a file only once expanded, returns the expanded
/// name. Like bash, if no file is named `text` itself, a leading `~` and parameters in its
/// directory part are expanded. `expanded_dirs` caches the expanded directories.
async fn expand_file_name(
    shell: &Shell<impl extensions::ShellExtensions>,
    text: &str,
    expanded_dirs: &mut HashMap<String, String>,
) -> Option<String> {
    let (dir, name) = split_dir(text);
    if !(dir.starts_with('~') || dir.contains(['$', '`']))
        || shell.absolute_path(Path::new(text)).exists()
    {
        return None;
    }

    let expanded_dir = if let Some(expanded_dir) = expanded_dirs.get(dir) {
        expanded_dir.clone()
    } else {
        let expanded_dir = expand_directory(shell, dir, DirExpansion::Dequoted).await;
        expanded_dirs.insert(dir.to_owned(), expanded_dir.clone());
        expanded_dir
    };

    (expanded_dir != dir).then(|| std::format!("{expanded_dir}{name}"))
}

/// Like bash's default completion, completes a word that's a glob pattern to the file name
/// it matches, if it matches exactly one. As in bash, the word is globbed as is: quote chars
/// in it are matched literally, and it's neither tilde- nor parameter-expanded.
pub(super) fn glob_completion(
    shell: &Shell<impl extensions::ShellExtensions>,
    word: &str,
) -> Option<String> {
    let extended_globbing = shell.options().extended_globbing;
    if !brush_parser::pattern::pattern_has_glob_metacharacters(word, extended_globbing) {
        return None;
    }

    let options = patterns::FilenameExpansionOptions {
        require_dot_in_pattern_to_match_dot_files: !shell.options().glob_matches_dotfiles,
    };
    let paths = shell_pattern(shell, word)
        .expand(
            shell.working_dir(),
            Some(&patterns::Pattern::accept_all_expand_filter),
            &options,
        )
        .ok()?
        .into_paths();

    let [path] = <[String; 1]>::try_from(paths).ok()?;
    Some(path)
}
