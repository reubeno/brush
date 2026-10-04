//! Paths resolved against a shell's working directory.

use std::path::{Component, Path, PathBuf, Prefix};

use normalize_path::NormalizePath as _;

/// A path resolved against a shell's working directory.
///
/// brush never changes the host process's working directory, so a relative [`Path`] handed
/// to the filesystem resolves against the wrong directory. A `ResolvedPath` doesn't depend on
/// the process's working directory, and filesystem operations are offered on it instead of on
/// [`Path`]. Get one from [`Shell::absolute_path`](crate::Shell::absolute_path).
///
/// An empty path stays empty: it names no file, so filesystem operations on it fail.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "PathBuf", into = "PathBuf")
)]
pub struct ResolvedPath(PathBuf);

// Each filesystem operation below is the one place brush-core calls its `Path` counterpart.
impl ResolvedPath {
    /// Returns the path as a [`Path`].
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Returns whether the path is empty. An empty path names no file; a shell has one as its
    /// working directory when it doesn't know where it is.
    pub fn is_empty(&self) -> bool {
        self.0.as_os_str().is_empty()
    }

    /// Returns the path as a [`PathBuf`].
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Resolves `path` against this one. An absolute `path` is taken as it is, and a relative
    /// one is joined onto this. A drive-relative path (Windows's `C:foo`) is relative to that
    /// drive's working directory, and the shell has only the one: on its drive, the path is
    /// joined onto it, and on any other, onto that drive's root. An empty path names no file,
    /// so the result is empty if `path` is, or if this is and `path` is relative.
    #[must_use]
    pub fn join(&self, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if path.is_absolute() {
            Self(path.to_owned())
        } else if path.as_os_str().is_empty() || self.0.as_os_str().is_empty() {
            Self::default()
        } else {
            Self(join_relative(&self.0, path))
        }
    }

    /// Normalizes the path lexically, without consulting the filesystem.
    #[must_use]
    pub(crate) fn normalized(&self) -> Self {
        Self(self.0.normalize())
    }

    /// See [`Path::exists`].
    pub fn exists(&self) -> bool {
        self.0.exists()
    }

    /// See [`Path::is_dir`].
    pub fn is_dir(&self) -> bool {
        self.0.is_dir()
    }

    /// See [`Path::is_file`].
    pub fn is_file(&self) -> bool {
        self.0.is_file()
    }

    /// See [`Path::is_symlink`].
    pub fn is_symlink(&self) -> bool {
        self.0.is_symlink()
    }

    /// See [`Path::metadata`].
    pub fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        self.0.metadata()
    }

    /// See [`Path::symlink_metadata`].
    pub fn symlink_metadata(&self) -> std::io::Result<std::fs::Metadata> {
        self.0.symlink_metadata()
    }

    /// See [`Path::read_dir`].
    pub fn read_dir(&self) -> std::io::Result<std::fs::ReadDir> {
        self.0.read_dir()
    }

    /// See [`Path::canonicalize`].
    pub fn canonicalize(&self) -> std::io::Result<Self> {
        self.0.canonicalize().map(Self)
    }

    /// Opens the file at this path with the given options; see [`std::fs::OpenOptions::open`].
    pub fn open(&self, options: &std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
        options.open(&self.0)
    }
}

impl AsRef<Path> for ResolvedPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl From<ResolvedPath> for PathBuf {
    fn from(path: ResolvedPath) -> Self {
        path.0
    }
}

/// Accepts a path that is already absolute (or empty), which needs no shell to resolve it.
impl TryFrom<PathBuf> for ResolvedPath {
    type Error = std::io::Error;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        if path.as_os_str().is_empty() || path.is_absolute() {
            Ok(Self(path))
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("not an absolute path: {}", path.display()),
            ))
        }
    }
}

/// Joins the relative `path` onto the absolute `base`; see [`ResolvedPath::join`].
fn join_relative(base: &Path, path: &Path) -> PathBuf {
    let mut components = path.components();
    if let Some(Component::Prefix(prefix)) = components.next()
        && !path.has_root()
    {
        let rest = components.as_path();
        let same_drive = matches!(
            base.components().next(),
            Some(Component::Prefix(base_prefix)) if same_drive(base_prefix.kind(), prefix.kind())
        );
        if same_drive {
            return base.join(rest);
        }
        let mut root = PathBuf::from(prefix.as_os_str());
        root.push(std::path::MAIN_SEPARATOR_STR);
        return root.join(rest);
    }
    base.join(path)
}

/// Returns whether two path prefixes name the same drive, whichever form each is in (`C:`,
/// `\\?\C:`).
const fn same_drive(a: Prefix<'_>, b: Prefix<'_>) -> bool {
    match (a, b) {
        (Prefix::Disk(a) | Prefix::VerbatimDisk(a), Prefix::Disk(b) | Prefix::VerbatimDisk(b)) => {
            a.eq_ignore_ascii_case(&b)
        }
        _ => false,
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use std::io::{Read as _, Write as _};

    use anyhow::Result;

    use super::*;

    /// A temporary directory, and a `ResolvedPath` for it.
    fn scratch() -> Result<(tempfile::TempDir, ResolvedPath)> {
        let dir = tempfile::tempdir()?;
        let path = ResolvedPath::try_from(dir.path().to_owned())?;
        Ok((dir, path))
    }

    #[test]
    fn joining_a_relative_path_appends_it() -> Result<()> {
        let (dir, base) = scratch()?;
        assert_eq!(base.join("a/b").as_path(), dir.path().join("a/b"));
        Ok(())
    }

    #[test]
    fn joining_an_absolute_path_replaces_the_base() -> Result<()> {
        let (_dir, base) = scratch()?;
        let (other, _) = scratch()?;
        assert_eq!(base.join(other.path()).as_path(), other.path());
        Ok(())
    }

    /// `..` is kept as written; joining doesn't consult the filesystem or normalize.
    #[test]
    fn joining_keeps_parent_components() -> Result<()> {
        let (dir, base) = scratch()?;
        assert_eq!(base.join("../x").as_path(), dir.path().join("../x"));
        Ok(())
    }

    /// An empty path names no file, so joining one stays empty rather than naming the base.
    #[test]
    fn joining_an_empty_path_is_empty() -> Result<()> {
        let (_dir, base) = scratch()?;
        assert!(base.join("").is_empty());
        Ok(())
    }

    /// An empty path names no file, so neither does anything joined onto it: the result
    /// mustn't be a relative path, which would resolve against the process's directory.
    #[test]
    fn joining_onto_an_empty_path_stays_resolved() -> Result<()> {
        let joined = ResolvedPath::default().join("x");
        assert!(joined.is_empty(), "{joined:?}");

        let (dir, _) = scratch()?;
        assert_eq!(
            ResolvedPath::default().join(dir.path()).as_path(),
            dir.path()
        );
        Ok(())
    }

    #[test]
    fn normalizing_removes_dot_and_parent_components() -> Result<()> {
        let (dir, base) = scratch()?;
        assert_eq!(
            base.join("a/./b/../c").normalized().as_path(),
            dir.path().join("a/c")
        );
        Ok(())
    }

    #[test]
    fn an_absolute_path_converts() -> Result<()> {
        let (dir, _) = scratch()?;
        let path = ResolvedPath::try_from(dir.path().to_owned())?;
        assert_eq!(path.as_path(), dir.path());
        Ok(())
    }

    #[test]
    fn an_empty_path_converts() -> Result<()> {
        let path = ResolvedPath::try_from(PathBuf::new())?;
        assert_eq!(path, ResolvedPath::default());
        Ok(())
    }

    #[test]
    fn a_relative_path_does_not_convert() {
        let error = ResolvedPath::try_from(PathBuf::from("a/b")).err();
        assert_eq!(
            error.map(|e| e.kind()),
            Some(std::io::ErrorKind::InvalidInput)
        );
    }

    #[test]
    fn conversions_expose_the_same_path() -> Result<()> {
        let (dir, path) = scratch()?;
        assert_eq!(path.as_path(), dir.path());
        assert_eq!(AsRef::<Path>::as_ref(&path), dir.path());
        assert_eq!(path.clone().into_path_buf(), dir.path());
        assert_eq!(PathBuf::from(path), dir.path());
        Ok(())
    }

    #[test]
    fn filesystem_queries_see_files_and_directories() -> Result<()> {
        let (_dir, base) = scratch()?;
        std::fs::create_dir(base.join("sub").as_path())?;
        std::fs::write(base.join("file").as_path(), "contents")?;

        let file = base.join("file");
        assert!(file.exists());
        assert!(file.is_file());
        assert!(!file.is_dir());
        assert!(!file.is_symlink());
        assert_eq!(file.metadata()?.len(), 8);
        assert_eq!(file.symlink_metadata()?.len(), 8);

        let sub = base.join("sub");
        assert!(sub.exists());
        assert!(sub.is_dir());
        assert!(!sub.is_file());

        let mut names = base
            .read_dir()?
            .map(|entry| Ok(entry?.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        names.sort();
        assert_eq!(names, ["file", "sub"]);

        Ok(())
    }

    #[test]
    fn missing_paths_do_not_exist() -> Result<()> {
        let (_dir, base) = scratch()?;
        let missing = base.join("missing");
        assert!(!missing.exists());
        assert!(!missing.is_file());
        assert!(!missing.is_dir());
        assert_eq!(
            missing.metadata().err().map(|e| e.kind()),
            Some(std::io::ErrorKind::NotFound)
        );
        Ok(())
    }

    /// An empty path names no file: every operation on it fails, rather than acting on the
    /// process's working directory.
    #[test]
    fn the_empty_path_names_no_file() {
        let empty = ResolvedPath::default();
        assert!(!empty.exists());
        assert!(!empty.is_dir());
        assert!(!empty.is_file());
        assert!(empty.metadata().is_err());
        assert!(empty.read_dir().is_err());
        assert!(empty.canonicalize().is_err());
        assert!(empty.open(std::fs::OpenOptions::new().read(true)).is_err());
    }

    #[test]
    fn canonicalizing_stays_resolved() -> Result<()> {
        let (dir, base) = scratch()?;
        std::fs::create_dir(base.join("sub").as_path())?;

        let canonical = base.join("sub/..").canonicalize()?;
        assert_eq!(canonical.as_path(), dir.path().canonicalize()?);
        assert!(canonical.as_path().is_absolute());
        Ok(())
    }

    #[test]
    fn opening_reads_and_writes() -> Result<()> {
        let (_dir, base) = scratch()?;
        let file = base.join("file");

        file.open(std::fs::OpenOptions::new().write(true).create_new(true))?
            .write_all(b"written")?;

        let mut contents = String::new();
        file.open(std::fs::OpenOptions::new().read(true))?
            .read_to_string(&mut contents)?;
        assert_eq!(contents, "written");
        Ok(())
    }
}
