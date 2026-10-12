//! Resolves what file-name candidates name.

use std::path::Path;

use super::{CandidateKind, ResolvedCandidate};
use crate::{Shell, extensions, sys};

/// Resolves candidates that are file names: whether each names a directory (which one
/// ending with a `/` is taken to be).
pub(super) fn resolve_file_names(
    shell: &Shell<impl extensions::ShellExtensions>,
    texts: Vec<String>,
) -> Vec<ResolvedCandidate> {
    texts
        .into_iter()
        .map(|text| {
            let is_dir = sys::fs::ends_with_path_separator(&text)
                || shell.absolute_path(Path::new(&text)).is_dir();
            ResolvedCandidate {
                text,
                kind: CandidateKind::FileName { is_dir },
            }
        })
        .collect()
}
