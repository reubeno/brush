//! Filesystem interaction in the shell.

use std::path::{Component, Path, PathBuf};

use crate::{
    ExecutionParameters, ShellFd,
    env::{EnvironmentLookup, EnvironmentScope},
    error, openfiles, pathsearch,
    sys::{fs::PathExt as _, users},
    variables,
};

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Sets the shell's current working directory to the given path.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The path to set as the working directory.
    pub fn set_working_dir(&mut self, target_dir: impl AsRef<Path>) -> Result<(), error::Error> {
        let abs_path = self.absolute_path(target_dir.as_ref());

        match abs_path.metadata() {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::ErrorKind::NotADirectory(abs_path.into()).into());
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // Normalize the path (but don't canonicalize it).
        let cleaned_path = abs_path.normalized();

        let pwd = cleaned_path.as_path().to_string_lossy().to_string();

        self.env.update_or_add(
            "PWD",
            variables::ShellValueLiteral::Scalar(pwd),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;
        let oldpwd = std::mem::replace(self.working_dir_mut(), cleaned_path);

        self.env.update_or_add(
            "OLDPWD",
            variables::ShellValueLiteral::Scalar(oldpwd.as_path().to_string_lossy().to_string()),
            |_| Ok(()),
            EnvironmentLookup::Anywhere,
            EnvironmentScope::Global,
        )?;

        Ok(())
    }

    /// Tilde-shortens the given string, replacing the user's home directory with a tilde.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to shorten.
    pub fn tilde_shorten(&self, s: String) -> String {
        if let Some(home_dir) = self.home_dir()
            && let Some(stripped) = s.strip_prefix(home_dir.to_string_lossy().as_ref())
        {
            return format!("~{stripped}");
        }
        s
    }

    /// Returns the shell's current home directory, if available.
    pub(crate) fn home_dir(&self) -> Option<PathBuf> {
        if let Some(home) = self.env.get_str("HOME", self) {
            Some(PathBuf::from(home.to_string()))
        } else {
            // HOME isn't set, so let's sort it out ourselves.
            users::get_current_user_home_dir()
        }
    }

    /// Finds executables with the given name in the shell's current PATH, yielding each match
    /// in search order. A match is named the way [`pathsearch`] describes.
    ///
    /// # Arguments
    ///
    /// * `filename` - The name of the executable to look for.
    pub fn find_executables_in_path<'a>(
        &'a self,
        filename: &'a str,
    ) -> impl Iterator<Item = PathBuf> + 'a {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let paths = crate::sys::fs::split_paths(path_var.as_ref());

        pathsearch::search_for_executable(self.working_dir(), paths, filename)
    }

    /// Finds executables in the shell's current default PATH, with filenames matching the
    /// given prefix.
    ///
    /// # Arguments
    ///
    /// * `filename_prefix` - The prefix to match against executable filenames.
    pub fn find_executables_in_path_with_prefix(
        &self,
        filename_prefix: &str,
        case_insensitive: bool,
    ) -> impl Iterator<Item = PathBuf> {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let paths = crate::sys::fs::split_paths(path_var.as_ref());

        pathsearch::search_for_executable_with_prefix(
            self.working_dir(),
            paths,
            filename_prefix,
            case_insensitive,
        )
    }

    /// Determines whether the given filename is the name of an executable in one of the
    /// directories in the shell's current PATH. If found, returns the path.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path<S: AsRef<str>>(
        &self,
        candidate_name: S,
    ) -> Option<PathBuf> {
        self.find_executables_in_path(candidate_name.as_ref())
            .next()
    }

    /// Uses the shell's hash-based path cache to check whether the given filename is the name
    /// of an executable in one of the directories in the shell's current PATH. If found,
    /// ensures the path is in the cache and returns it.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the file to look for.
    pub fn find_first_executable_in_path_using_cache<S: AsRef<str>>(
        &mut self,
        candidate_name: S,
    ) -> Option<PathBuf>
    where
        String: From<S>,
    {
        if let Some(cached_path) = self.hashed_command_path(candidate_name.as_ref()) {
            Some(cached_path)
        } else if let Some(found_path) = self.find_first_executable_in_path(&candidate_name) {
            self.program_location_cache
                .set(candidate_name, found_path.clone());
            Some(found_path)
        } else {
            None
        }
    }

    /// Looks a command name up in the shell's hash table, the way bash does. A hashed relative
    /// path is relative to the working directory: if it names an executable file there, it's
    /// returned with a leading `./`, so it isn't taken for a name to search `PATH` for. If it
    /// doesn't, a path that already had the `./` counts as not hashed, and any other is
    /// returned as stored.
    ///
    /// # Arguments
    ///
    /// * `name` - The command name to look up.
    pub fn hashed_command_path(&self, name: &str) -> Option<PathBuf> {
        let path = self.program_location_cache.get(name)?;
        hashed_path_in(path, self.working_dir())
    }

    /// Resolves a command name by searching the shell's current PATH. A match is named the way
    /// [`pathsearch`] describes.
    ///
    /// Unlike [`Self::find_first_executable_in_path`], a non-executable entry in the PATH
    /// resolves as the command; the shell reports it as the command and then fails to run it.
    /// See [`pathsearch::resolve_command`].
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the command to resolve.
    pub fn resolve_command_in_path<S: AsRef<str>>(&self, candidate_name: S) -> Option<PathBuf> {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        let paths = crate::sys::fs::split_paths(path_var.as_ref());
        pathsearch::resolve_command(self.working_dir(), paths, candidate_name.as_ref())
    }

    /// Like [`Self::resolve_command_in_path`], but consults the shell's hash-based path cache
    /// first and caches whatever a search turns up.
    ///
    /// # Arguments
    ///
    /// * `candidate_name` - The name of the command to resolve.
    pub fn resolve_command_in_path_using_cache<S: AsRef<str>>(
        &mut self,
        candidate_name: S,
    ) -> Option<PathBuf>
    where
        String: From<S>,
    {
        if let Some(cached_path) = self.hashed_command_path(candidate_name.as_ref()) {
            return Some(cached_path);
        }

        let found_path = self.resolve_command_in_path(candidate_name.as_ref())?;
        self.program_location_cache
            .set(candidate_name, found_path.clone());

        Some(found_path)
    }

    /// Resolves the given path against the shell's working directory. Filesystem operations
    /// must go through the result rather than through a relative path, which the filesystem
    /// would resolve against the host process's working directory instead.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to resolve.
    pub fn absolute_path(&self, path: impl AsRef<Path>) -> crate::ResolvedPath {
        self.working_dir().join(path)
    }

    /// Opens the given file, using the context of this shell and the provided execution parameters.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use opening the file.
    /// * `path` - The path to the file to open; may be relative to the shell's working directory.
    /// * `params` - Execution parameters.
    pub(crate) fn open_file(
        &self,
        options: &std::fs::OpenOptions,
        path: impl AsRef<Path>,
        params: &ExecutionParameters,
    ) -> Result<openfiles::OpenFile, std::io::Error> {
        // Give platform-specific code a chance to handle special files
        // (e.g. /dev/null on Windows, which needs to open NUL instead).
        // This is checked before absolute_path so that paths like /dev/null
        // are intercepted on platforms where they aren't valid native paths.
        if let Some(result) = crate::sys::fs::try_open_special_file(path.as_ref()) {
            return result.map(openfiles::OpenFile::from);
        }

        let path_to_open = self.absolute_path(path.as_ref());

        // See if this is a reference to a file descriptor. These paths should
        // reflect the shell's current execution fds, which can differ from the
        // host process fds after redirections like here-docs.
        if let Some(fd_num) = shell_fd_path_to_fd(path_to_open.as_path())
            && let Some(open_file) = params.try_fd(self, fd_num)
        {
            return Ok(open_file);
        }

        Ok(path_to_open.open(options)?.into())
    }

    /// Replaces the shell's currently configured open files with the given set.
    /// Typically only used by exec-like builtins.
    ///
    /// # Arguments
    ///
    /// * `open_files` - The new set of open files to use.
    pub fn replace_open_files(
        &mut self,
        open_fds: impl Iterator<Item = (ShellFd, openfiles::OpenFile)>,
    ) {
        self.open_files = openfiles::OpenFiles::from(open_fds);
    }

    pub(crate) const fn persistent_open_files(&self) -> &openfiles::OpenFiles {
        &self.open_files
    }
}

/// Interprets a hashed `path` relative to `working_dir`; see [`crate::Shell::hashed_command_path`].
fn hashed_path_in(path: PathBuf, working_dir: &crate::ResolvedPath) -> Option<PathBuf> {
    if path.is_absolute() {
        return Some(path);
    }

    let dotted = path.components().next() == Some(Component::CurDir);
    let candidate = if dotted {
        path.clone()
    } else {
        Path::new(".").join(&path)
    };

    let file = working_dir.join(&candidate);
    if !file.is_dir() && file.executable() {
        Some(candidate)
    } else if dotted {
        None
    } else {
        Some(path)
    }
}

fn shell_fd_path_to_fd(path: &Path) -> Option<ShellFd> {
    match path.to_str()? {
        "/dev/stdin" => return Some(openfiles::OpenFiles::STDIN_FD),
        "/dev/stdout" => return Some(openfiles::OpenFiles::STDOUT_FD),
        "/dev/stderr" => return Some(openfiles::OpenFiles::STDERR_FD),
        _ => {}
    }

    if let Some(parent) = path.parent()
        && parent == Path::new("/dev/fd")
        && let Some(filename) = path.file_name()
    {
        filename.to_string_lossy().parse::<ShellFd>().ok()
    } else {
        None
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use anyhow::Result;

    use super::*;

    /// The executable [`lookups`] creates, with the platform's executable suffix.
    fn prog() -> String {
        format!("bin/prog{}", std::env::consts::EXE_SUFFIX)
    }

    /// Looks each relative path up as if hashed, from a directory holding an executable
    /// [`prog`], a non-executable `data`, and a directory `dir`.
    fn lookups(paths: &[&str]) -> Result<Vec<Option<PathBuf>>> {
        let scratch = tempfile::tempdir()?;
        std::fs::create_dir_all(scratch.path().join("bin"))?;
        std::fs::create_dir_all(scratch.path().join("dir"))?;
        let prog = scratch.path().join(prog());
        std::fs::write(&prog, "")?;
        crate::sys::fs::make_executable(&prog)?;
        std::fs::write(scratch.path().join("data"), "")?;
        let working_dir = crate::ResolvedPath::try_from(scratch.path().to_owned())?;

        Ok(paths
            .iter()
            .map(|path| hashed_path_in(PathBuf::from(path), &working_dir))
            .collect())
    }

    #[test]
    fn executable_relative_path_gets_a_leading_dot() -> Result<()> {
        let prog = prog();
        let dotted = format!("./{prog}");
        let indirect = format!("dir/../{prog}");
        assert_eq!(
            lookups(&[&prog, &dotted, &indirect])?,
            [
                Some(Path::new(".").join(&prog)),
                Some(PathBuf::from(&dotted)),
                Some(Path::new(".").join(&indirect)),
            ]
        );
        Ok(())
    }

    /// Only a leading `.` component counts as already dotted: `.hidden` and `..` don't.
    #[test]
    fn dot_prefix_is_a_whole_component() -> Result<()> {
        assert_eq!(
            lookups(&["../missing", ".missing"])?,
            [
                Some(PathBuf::from("../missing")),
                Some(PathBuf::from(".missing"))
            ]
        );
        Ok(())
    }

    #[test]
    fn other_relative_paths_are_returned_as_stored() -> Result<()> {
        assert_eq!(
            lookups(&["data", "dir", "missing"])?,
            [
                Some(PathBuf::from("data")),
                Some(PathBuf::from("dir")),
                Some(PathBuf::from("missing")),
            ]
        );
        Ok(())
    }

    #[test]
    fn dotted_path_that_is_not_executable_is_not_hashed() -> Result<()> {
        assert_eq!(
            lookups(&["./data", "./dir", "./missing"])?,
            [None, None, None]
        );
        Ok(())
    }

    #[test]
    fn absolute_path_is_returned_as_stored() {
        let path = std::env::temp_dir().join("prog");
        assert_eq!(
            hashed_path_in(path.clone(), &crate::ResolvedPath::default()),
            Some(path)
        );
    }
}
