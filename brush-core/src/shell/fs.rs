//! Filesystem interaction in the shell.

use std::path::{Path, PathBuf};

use normalize_path::NormalizePath as _;

use crate::{
    ExecutionParameters, ShellFd,
    env::{EnvironmentLookup, EnvironmentScope},
    error, openfiles, pathsearch,
    sys::{fs::PathExt as _, users},
    variables,
};

/// Where the name of a command (one without a path separator) leads.
enum Located {
    /// The shell's hash table has a path for it.
    Hashed(PathBuf),
    /// A search of `PATH` found it.
    Found(PathBuf),
}

impl<SE: crate::extensions::ShellExtensions> crate::Shell<SE> {
    /// Sets the shell's current working directory to the given path.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The path to set as the working directory.
    pub fn set_working_dir(&mut self, target_dir: impl AsRef<Path>) -> Result<(), error::Error> {
        let abs_path = self.absolute_path(target_dir.as_ref());

        match std::fs::metadata(&abs_path) {
            Ok(m) => {
                if !m.is_dir() {
                    return Err(error::ErrorKind::NotADirectory(abs_path).into());
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // Normalize the path (but don't canonicalize it).
        let cleaned_path = abs_path.normalize();

        let pwd = cleaned_path.to_string_lossy().to_string();

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
            variables::ShellValueLiteral::Scalar(oldpwd.to_string_lossy().to_string()),
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

    /// Returns the shell's command search path: its current `PATH`, with relative
    /// directories relative to its working directory. Searching it doesn't consult or
    /// change the shell's hash table; see [`Self::resolve_external_command`] and
    /// [`Self::hash_command`] for that.
    pub fn command_search_path(&self) -> pathsearch::SearchPath {
        let path_var = self.env.get_str("PATH", self).unwrap_or_default();
        pathsearch::SearchPath::new(
            crate::sys::fs::split_paths(path_var.as_ref()),
            self.working_dir(),
        )
    }

    /// Looks up where the name of a command leads, if not to a path: the shell's hash
    /// table, or else a search of `PATH` (see [`pathsearch::SearchPath::resolve`]).
    fn locate_command(&self, name: &str) -> Option<Located> {
        if let Some(path) = self.program_location_cache.get(name) {
            return Some(Located::Hashed(path));
        }

        self.command_search_path().resolve(name).map(Located::Found)
    }

    /// Looks up the program an external command's name refers to, as running the
    /// command would find it, but without hashing it (as `type` and `command -v` do).
    ///
    /// A name with a path separator is a path, relative to the working directory, which
    /// is a command only if it's executable and not a directory. Any other is looked up
    /// in the shell's hash table, or else searched for in `PATH` (see
    /// [`pathsearch::SearchPath::resolve`]).
    ///
    /// Returns the program's path (relative to the working directory, if it's
    /// relative), and whether it came from the hash table.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    pub fn lookup_external_command(&self, name: &str) -> Option<(PathBuf, bool)> {
        if crate::sys::fs::contains_path_separator(name) {
            // A directory is never a command, even though it carries the execute bit.
            let candidate = self.absolute_path(Path::new(name));
            return (!candidate.is_dir() && candidate.executable())
                .then(|| (PathBuf::from(name), false));
        }

        match self.locate_command(name)? {
            Located::Hashed(path) => Some((path, true)),
            Located::Found(path) => Some((path, false)),
        }
    }

    /// Resolves the name of an external command to the program to run, as running the
    /// command would.
    ///
    /// A name with a path separator is a path, relative to the working directory. Any
    /// other is looked up in the shell's hash table, or else searched for in `PATH`
    /// (see [`pathsearch::SearchPath::resolve`]), and what the search finds is hashed.
    ///
    /// Returns the program's path, relative to the working directory if it's relative.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    pub fn resolve_external_command(&mut self, name: &str) -> Option<PathBuf> {
        if crate::sys::fs::contains_path_separator(name) {
            return Some(crate::sys::fs::resolve_executable_path(
                PathBuf::from(name),
                self.working_dir(),
            ));
        }

        match self.locate_command(name)? {
            Located::Hashed(path) => Some(path),
            Located::Found(path) => {
                self.program_location_cache.set(name, path.clone());
                Some(path)
            }
        }
    }

    /// Hashes the command with the given name, as `hash name` does. Whatever the shell's
    /// hash table had for the name is forgotten. If the name is a path, or a function's
    /// or a builtin's, nothing takes its place; otherwise `PATH` is searched for an
    /// executable of the name (see [`pathsearch::SearchPath::find`]), which is hashed.
    ///
    /// Returns whether the name is a command: only a name that's searched for and not
    /// found isn't.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the command.
    pub fn hash_command(&mut self, name: &str) -> bool {
        // Whatever was there goes, even if nothing's found to take its place.
        self.program_location_cache.unset(name);

        if crate::sys::fs::contains_path_separator(name)
            || self.funcs().get(name).is_some()
            || self.builtins().get(name).is_some_and(|b| !b.disabled)
        {
            return true;
        }

        let Some(path) = self.command_search_path().find(name) else {
            return false;
        };
        self.program_location_cache.set(name, path);
        true
    }

    /// Gets the absolute form of the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to get the absolute form of.
    pub fn absolute_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.as_os_str().is_empty() || path.is_absolute() {
            path.to_owned()
        } else {
            self.working_dir().join(path)
        }
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
        if let Some(fd_num) = shell_fd_path_to_fd(&path_to_open)
            && let Some(open_file) = params.try_fd(self, fd_num)
        {
            return Ok(open_file);
        }

        Ok(options.open(path_to_open)?.into())
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
