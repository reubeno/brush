use std::{collections::VecDeque, path::PathBuf};

use crate::sys::fs::PathExt;

pub(crate) struct ExecutablePathSearch<PI, N>
where
    PI: AsRef<str>,
    N: AsRef<str>,
{
    paths: VecDeque<PI>,
    filename: N,
}

impl<PI, N> Iterator for ExecutablePathSearch<PI, N>
where
    PI: AsRef<str>,
    N: AsRef<str>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.paths.pop_front() {
            let path = PathBuf::from(path.as_ref()).join(self.filename.as_ref());
            if path.is_file() && path.as_path().executable() {
                return Some(path);
            }
        }

        None
    }
}

pub(crate) struct ExecutablePathPrefixSearch<PI>
where
    PI: AsRef<str>,
{
    paths: VecDeque<PI>,
    queued_items: VecDeque<PathBuf>,
    filename_prefix: String,
    case_insensitive: bool,
}

impl<PI> Iterator for ExecutablePathPrefixSearch<PI>
where
    PI: AsRef<str>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        // If we already found some items and queued them, then yield one now.
        if let Some(item) = self.queued_items.pop_front() {
            return Some(item);
        }

        while let Some(path) = self.paths.pop_front() {
            let path = PathBuf::from(path.as_ref());

            if let Ok(readdir) = path.read_dir() {
                for entry in readdir.flatten() {
                    if let Ok(mut filename) = entry.file_name().into_string() {
                        if self.case_insensitive {
                            filename = filename.to_ascii_lowercase();
                        }

                        if !filename.starts_with(&self.filename_prefix) {
                            continue;
                        }
                    }

                    let entry_path = entry.path();

                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() && entry_path.executable() {
                            self.queued_items.push_back(entry_path);
                        }
                    }
                }
            }

            if let Some(item) = self.queued_items.pop_front() {
                return Some(item);
            }
        }

        None
    }
}

pub(crate) fn search_for_executable<P, PI, N>(paths: P, filename: N) -> ExecutablePathSearch<PI, N>
where
    P: Iterator<Item = PI>,
    PI: AsRef<str>,
    N: AsRef<str>,
{
    ExecutablePathSearch {
        paths: paths.collect(),
        filename,
    }
}

pub(crate) fn search_for_executable_with_prefix<P, PI>(
    paths: P,
    filename_prefix: &str,
    case_insensitive: bool,
) -> ExecutablePathPrefixSearch<PI>
where
    P: Iterator<Item = PI>,
    PI: AsRef<str>,
{
    let stored_prefix = if case_insensitive {
        filename_prefix.to_ascii_lowercase()
    } else {
        filename_prefix.into()
    };

    ExecutablePathPrefixSearch {
        paths: paths.collect(),
        queued_items: VecDeque::new(),
        filename_prefix: stored_prefix,
        case_insensitive,
    }
}
