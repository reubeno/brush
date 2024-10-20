#[allow(unused_imports)]
pub(crate) use super::platform::fs::*;

use std::borrow::Cow;
use std::ffi::OsStr;
use std::ops::Deref;
#[cfg(unix)]
pub(crate) use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
#[cfg(not(unix))]
pub(crate) use StubMetadataExt as MetadataExt;

pub(crate) trait PathExt {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn executable(&self) -> bool;

    fn exists_and_is_block_device(&self) -> bool;
    fn exists_and_is_char_device(&self) -> bool;
    fn exists_and_is_fifo(&self) -> bool;
    fn exists_and_is_socket(&self) -> bool;
    fn exists_and_is_setgid(&self) -> bool;
    fn exists_and_is_setuid(&self) -> bool;
    fn exists_and_is_sticky_bit(&self) -> bool;
}

/// An error returned from [`AbsolutePath::from_absolute`] if the path is not absolute.
#[derive(Debug)]
pub struct IsNotAbsoluteError<P>(P);

/// A wrapper around [`std::path::Path`] to indicate that certain
/// functions, such as [`normalize_lexically`], require an absolute path to work correctly.
pub struct AbsolutePath<'a>(
    Cow<'a, Path>, /* May hold either `&Path` or `PathBuf` */
);
impl<'a> AbsolutePath<'a> {
    /// Consumes the [`AbsolutePath`], yielding its internal [`Cow<'a, Path>`](Cow) storage.
    pub fn into_inner(self) -> Cow<'a, Path> {
        self.0
    }

    /// Constructs an absolute path from the given `path` relative to the base `relative_to`
    ///
    /// Uses [`make_absolute`]  to construct the absolute path.
    /// See its documentation for more.
    ///
    /// # Arguments
    ///
    /// - `relative_to` - A base path (similar to `cwd`) which the `path` is relative to.
    /// - `path` - A [`TildaExpandedPath`] to make absolute.
    pub fn new<R>(relative_to: R, path: TildaExpandedPath<'a>) -> Self
    where
        std::path::PathBuf: From<R>,
        Cow<'a, Path>: From<R>,
        std::path::PathBuf: From<&'a str>,
        R: AsRef<Path>,
    {
        AbsolutePath(make_absolute(relative_to, path))
    }

    /// Constructs [`AbsolutePath`] from any path that is already absolute; otherwise,
    /// returns an error [`IsNotAbsoluteError`].
    ///
    /// # Arguments
    ///
    /// - `path` - An absolute absolute.
    pub fn try_from_absolute<P>(path: P) -> Result<Self, IsNotAbsoluteError<P>>
    where
        P: AsRef<Path>,
        Cow<'a, Path>: From<P>,
    {
        if path.as_ref().is_absolute() {
            Ok(AbsolutePath(Cow::from(path)))
        } else {
            Err(IsNotAbsoluteError(path))
        }
    }
}

impl AsRef<Path> for AbsolutePath<'_> {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}
impl Deref for AbsolutePath<'_> {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A Type to explicitly indicate that the path doesn't contait a telde '~'.
///
/// This `struct` is created by the [`expand_tilde_with_home`] function.
pub struct TildaExpandedPath<'a>(Cow<'a, Path>);
impl<'a> TildaExpandedPath<'a> {
    pub fn into_inner(self) -> Cow<'a, Path> {
        self.0
    }
}

/// Makes `path` absolute using `relative_to` as the base.
///
/// Does nothing if the path is already absolute.
///
/// Note that the function requires the [`TildaExpandedPath`] as the `path` created by
/// [`expand_tilde_with_home`] because otherwise the result could end up as "/some/path/~" or
/// "/some/path/~user".
///
/// # Arguments
///
/// - `relative_to` - A base path (similar to `cwd`) which the `path` is relative to.
/// - `path` - A [`TildaExpandedPath`] to make absolute.
pub fn make_absolute<'a, R>(relative_to: R, path: TildaExpandedPath<'a>) -> Cow<'a, Path>
where
    // If `R` is a `&Path` convert it to `Path::to_path_buf()` only if nessesarry, if it is a `PathBuf`,
    // return the argument unchanged without additional allocations.
    std::path::PathBuf: From<R>,
    std::path::PathBuf: From<&'a str>,
    R: AsRef<Path>,
    Cow<'a, Path>: From<R>,
{
    let path = path.into_inner();

    // Windows verbatim paths should not be modified.
    if path.as_ref().is_absolute() || is_verbatim(&path) {
        path
    } else {
        if path.as_ref().as_os_str().as_encoded_bytes() == b"." {
            // Joining a Path with '.' appends a '.' at the end,
            // so we don't do anything, which should result in an equal
            // path on all supported systems.
            return relative_to.into();
        }
        relative_join(relative_to, &path).into()
    }
}

/// Creates a new path where:
/// - Multiple `/`'s are collapsed to a single `/`.
/// - Leading `./`'s and trailing `/.`'s are removed.
/// - `../`'s are handled by removing portions of the path.
///
/// Note that unlike [`std::fs::canonicalize`], this function:
/// - doesn't use syscalls (such as `readlink`).
/// - doesn't convert to absolute form.
/// - doesn't resolve symlinks.
/// - Does not check's if path actually exists.
///
/// Because of this, the function strictly requires an absolute path.
///
/// # Arguments
///
/// - `path` - An [`AbsolutePath`].
pub fn normalize_lexically(path: AbsolutePath<'_>) -> Cow<'_, Path> {
    let path = path.into_inner();

    if is_normalized(&path) {
        return path;
    }
    // NOTE: This is mostly taken from std::path:absolute, except we don't use
    // [`std::env::current_dir()`] here

    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut components = path.components();
    let path_os = path.as_os_str().as_encoded_bytes();

    let mut normalized = {
        // https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap04.html#tag_04_13
        // Posix: "If a pathname begins with two successive <slash> characters, the
        // first component following the leading <slash> characters may be
        // interpreted in an implementation-defined manner, although more than
        // two leading <slash> characters shall be treated as a single <slash>
        // character."
        #[cfg(unix)]
        {
            if path_os.starts_with(b"//") && !path_os.starts_with(b"///") {
                components.next();
                PathBuf::from("//")
            } else {
                PathBuf::new()
            }
        }
        #[cfg(not(unix))]
        {
            PathBuf::new()
        }
    };

    for component in components {
        match component {
            Component::Prefix(..) => {
                // The Windows prefix here such as C:/, unc or verbatim.
                // On Unix, C:/ is not allowed because such a path is considered non-absolute and
                // will be rejected by [`AbsolutePath`] API."
                #[cfg(windows)]
                {
                    normalized.push(component.as_os_str())
                }
                #[cfg(not(windows))]
                {
                    unreachable!()
                }
            }
            Component::RootDir => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(c) => {
                normalized.push(c);
            }
        }
    }

    // An empty result is really the root path because we've started with an absolute path
    if normalized.as_os_str().is_empty() {
        normalized.push(
            // at least one component, since we've already checked the path for emptiness
            path.components().next().unwrap(),
        );
    }

    // "Interfaces using pathname resolution may specify additional constraints
    // when a pathname that does not name an existing directory contains at
    // least one non- <slash> character and contains one or more trailing
    // <slash> characters".
    // A trailing <slash> is also meaningful if "a symbolic link is
    // encountered during pathname resolution".
    if path_os.ends_with(b"/") || path_os.ends_with(std::path::MAIN_SEPARATOR_STR.as_bytes()) {
        normalized.push("");
    }
    Cow::from(normalized)
}

// A verbatim `\\?\` path means that no normalization must be performed.
// Paths prefixed with `\\?\` are passed (almost) directly to the Windows kernel without any
// transformations or substitutions.
fn is_verbatim(path: &Path) -> bool {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => prefix.kind().is_verbatim(),
        _ => false,
    }
}

// Checks if the path is already normalized and whether [`normalize_lexically`] can be skipped.
// A path considered normalized if it is:
// - empty
// - a verbatim path on Windows
// - ends with `/` or additionally `\` on Windows
// - doesn't contain `.` and `..`
// - doesn't have multiple path separators (e.g., a//b)
fn is_normalized(path: &Path) -> bool {
    let path_os = path.as_os_str().as_encoded_bytes();

    if path.as_os_str().is_empty() {
        return true;
    }

    #[cfg(windows)]
    if is_verbatim(path) {
        return true;
    }

    // require ending `/`
    if !(path_os.ends_with(b"/")
        // check '\'
        || (cfg!(windows) && path_os.ends_with(std::path::MAIN_SEPARATOR_STR.as_bytes())))
    {
        return false;
    }

    // does not have any of `.`, `..`
    if path
        .components()
        .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
    {
        return false;
    }

    // contains any of the doubled slashes, such as a/b//d.
    if path.as_os_str().len() > 1 {
        // Skip the first `//` in POSIX, but not when the first is `///`.
        // skip the first \\ or // in Windows UNC and Device paths
        !path_os[1..]
            .windows(2)
            .any(|window| window == b"//" || (cfg!(windows) && window == br"\\"))
    } else {
        true
    }
}

/// Performs tilde expansion.
/// Returns a [`TildaExpandedPath`] type that indicates the path is expanded and ready for further
/// processing.
///
/// # Arguments
///
/// - `path` - A path to expand `~`.
/// - `home` - A path that `~` should be expanded to.
pub fn expand_tilde_with_home<'a, P, H>(path: &'a P, home: H) -> TildaExpandedPath<'a>
where
    std::path::PathBuf: From<H>,
    H: AsRef<Path> + 'a,
    P: AsRef<Path> + ?Sized,
    Cow<'a, Path>: From<&'a Path>,
    Cow<'a, Path>: From<H>,
{
    // let path = path.as_ref();
    let mut components = path.as_ref().components();
    let path = match components.next() {
        Some(Component::Normal(p)) if p.as_encoded_bytes() == b"~" => components.as_path(),
        // is already expanded
        _ => return TildaExpandedPath(path.as_ref().into()),
    };

    if home.as_ref().as_os_str().is_empty() || home.as_ref().as_os_str().as_encoded_bytes() == b"/"
    {
        // Corner case: `home` is a root directory;
        // don't prepend extra `/`, just drop the tilde.
        return TildaExpandedPath(Cow::from(path));
    }

    // Corner case: `p` is empty;
    // Don't append extra '/', just keep `home` as is.
    // This happens because PathBuf.push will always
    // add a separator if the pushed path is relative,
    // even if it's empty
    if path.as_os_str().as_encoded_bytes().is_empty() {
        return TildaExpandedPath(home.into());
    }
    let mut home = PathBuf::from(home);
    home.push(path);
    TildaExpandedPath(home.into())
}

/// A wrapper around [`Path::join`] with additional handling of the Windows's volume relative
/// paths (e.g `C:file` - A relative path from the current directory of the C: drive.)
// https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats
fn relative_join<C>(base: C, path: &Path) -> PathBuf
where
    std::path::PathBuf: From<C>,
    for<'a> std::path::PathBuf: From<&'a OsStr>,
    C: AsRef<Path>,
{
    #[cfg(windows)]
    if let (Some(Component::Prefix(cwd_prefix)), Some(Component::Prefix(path_prefix))) =
        (base.as_ref().components().next(), path.components().next())
    {
        let path = path.strip_prefix(path_prefix.as_os_str()).unwrap();
        // C:\cwd + C:data -> C:\cwd\data
        if path_prefix == cwd_prefix {
            let cwd = PathBuf::from(base);
            return cwd.join(path);
        }
        // C:\cwd + D:data -> D:\data
        let mut rtn = PathBuf::from(path_prefix.as_os_str());
        rtn.reserve(std::path::MAIN_SEPARATOR_STR.len() + path.as_os_str().len());
        rtn.push(std::path::MAIN_SEPARATOR_STR);
        rtn.push(path);
        return rtn;
    }
    let cwd = PathBuf::from(base);
    cwd.join(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_normalized() {
        let tests_normalized: &[&[&str]] = &[
            &[
                "/aa/bb/cc/dd/",
                #[cfg(unix)]
                "//aa/special/posix/case/",
            ],
            #[cfg(windows)]
            &[
                "C:/aa/bb/",
                r"C:\aa\bb\",
                r"C:\aa/b/",
                r"\\?\pictures\..\kittens",
                r"\\?\UNC\server\share",
                r"\\?\c$",
                r"\\.\pictures\kittens\",
                r"//.\pictures\kittens\",
                r"\\.\UNC\server\share\",
                r"\\server\share/",
            ],
        ];

        let tests_not_normalized: &[&[&str]] = &[
            &[
                "/aa/bb/cc/dd",
                "/aa/bb/../cc/dd",
                "///aa/bb/cc/dd",
                "///////aa/bb/cc/dd",
                "./aa/bb/cc/dd",
                "/aa/bb//cc/dd",
                "/aa/bb////cc/./dd",
                "/aa/bb////cc/dd",
                r"\\.\pictures\..\kittens",
                r"\\.\UNC\server\share",
                r"\\server\share",
            ],
            #[cfg(windows)]
            &["C:/aa/bb", r"C:\\\aa\bb\", r"C:\aa///b/"],
        ];

        for test in tests_normalized.into_iter().map(|s| *s).flatten() {
            assert!(is_normalized(&Path::new(test)), "{}", test);
        }
        for test in tests_not_normalized.into_iter().map(|s| *s).flatten() {
            assert!(!is_normalized(&Path::new(test)), "{}", test);
        }
    }

    #[test]
    fn test_make_absolute() {
        let home = Path::new(if cfg!(unix) {
            "/home"
        } else {
            r"C:\Users\Home\"
        });

        let cwd = Path::new(if cfg!(unix) { "/cwd" } else { r"C:\cwd" });

        let tests: &[&[(&str, &str)]] = &[
            #[cfg(unix)]
            &[
                ("~/aa/bb/", "/home/aa/bb"),
                ("./", "/cwd/"),
                (".", "/cwd/"),
                #[cfg(unix)]
                ("//the/absolute/posix", "//the/absolute/posix"),
            ],
            #[cfg(windows)]
            &[
                ("~/aa/bb/", r"C:\Users\Home\aa/bb"),
                ("./", r"C:\cwd\"),
                (".", r"C:\cwd\"),
                // super dumb relative paths
                ("Z:my_folder", r"Z:\my_folder"),
                ("Z:", r"Z:\"),
                ("C:my_folder", r"C:\cwd\my_folder"),
                // verbatim and unc
                (r"\\server\share\.\da\..\f\", r"\\server\share\.\da\..\f\"),
                (r"\\?\pics\..\of\./kittens", r"\\?\pics\..\of\./kittens"),
                (r"\\?\UNC\ser\share\data\..\", r"\\?\UNC\ser\share\data\..\"),
                (r"\\?\c:\..\..\..\..\../", r"\\?\c:\..\..\..\..\../"),
                (r"\\.\PIPE\name\../surname", r"\\.\PIPE\name\../surname/"),
                (r"\\server\share\..\data", r"\\server\share\..\data\"),
            ],
        ];

        for test in tests.into_iter().map(|s| *s).flatten() {
            assert_eq!(
                make_absolute(cwd, expand_tilde_with_home(&Path::new(test.0), home)),
                Path::new(test.1)
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_normalize_lexically() {
        let tests = vec![
            ("/", "/"),
            ("//", "//"),
            ("///", "/"),
            ("/.//", "/"),
            ("//..", "/"),
            ("/..//", "/"),
            ("/..//", "/"),
            ("/.//./", "/"),
            ("/././/./", "/"),
            ("/./././", "/"),
            ("/path//to///thing", "/path/to/thing"),
            ("/aa/bb/../cc/dd", "/aa/cc/dd"),
            ("/../aa/bb/../../cc/dd", "/cc/dd"),
            ("/../../../../aa/bb/../../cc/dd", "/cc/dd"),
            ("/aa/bb/../../cc/dd/../../../../../../../../../", "/"),
            ("/../../../../../../..", "/"),
            ("/../../../../../...", "/..."),
            ("/test/./path/", "/test/path"),
            ("/test/../..", "/"),
            ("/./././", "/"),
            ("///root/../home", "/home"),
            #[cfg(unix)]
            ("//root/../home", "//home"),
        ];

        for test in tests {
            assert_eq!(
                normalize_lexically(AbsolutePath::try_from_absolute(Path::new(test.0)).unwrap()),
                Path::new(test.1)
            );
            assert_eq!(
                normalize_lexically(AbsolutePath::new(
                    Path::new(test.0),
                    expand_tilde_with_home(&Path::new(test.0), Path::new("/home"))
                )),
                Path::new(test.1)
            );
        }

        // empty path is a and empty path
        assert_eq!(
            normalize_lexically(AbsolutePath::new(
                Path::new(""),
                TildaExpandedPath(Cow::from(Path::new("")))
            )),
            Path::new("")
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_normalize_lexically_windows() {
        let tests = vec![
            (r"C:\..", r"C:\"),
            (r"C:\../..\..\..\..", r"C:\"),
            (r"C:\..\test", r"C:\test"),
            (r"C:\test\..", r"C:\"),
            (r"C:\test\path\..\..\..", r"C:\"),
            (r"C:\test\path/..\../another\\path", r"C:\another\path"),
            (r"C:\test\path\\my/path", r"C:\test\path\my\path"),
            (r"C:/dir\../otherDir/test.json", "C:/otherDir/test.json"),
            (r"c:\test\..", r"c:\"),
            ("c:/test/..", "c:/"),
            (r"\\server\share\.\data\..\file\", r"\\server\share\file\"),
            // any of the verbatim paths should stay unchanged
            (r"\\?\pics\..\of\./kittens", r"\\?\pics\..\of\./kittens"),
            (r"\\?\UNC\ser\share\data\..\", r"\\?\UNC\ser\share\data\..\"),
            (r"\\?\c:\..\..\..\..\../", r"\\?\c:\..\..\..\..\../"),
            // other windows stuff
            (r"\\.\PIPE\name\../surname/", r"\\.\PIPE\surname\"),
            (r"\\.\PIPE\remove_all\..\..\..\..\..\..\", r"\\.\PIPE\"),
            // server\share is a part of the prefix
            (r"\\server\share\..\data\.\", r"\\server\share\data\"),
            (r"Z:\", r"Z:\"),
        ];

        for test in tests {
            assert_eq!(
                normalize_lexically(
                    AbsolutePath::try_from_absolute(PathBuf::from(test.0)).unwrap()
                ),
                PathBuf::from(test.1)
            );
        }
    }

    #[test]
    fn test_expand_tilde() {
        let home = Path::new(if cfg!(unix) {
            "/home"
        } else {
            r"C:\Users\Home\"
        });
        let check_expanded = |s: &str| {
            assert!(expand_tilde_with_home(Path::new(s), home)
                .into_inner()
                .starts_with(home));

            // Tests the special case in expand_tilde for "/" as home
            let home = Path::new("/");
            assert!(!expand_tilde_with_home(Path::new(s), home)
                .into_inner()
                .starts_with("//"));
        };

        let check_not_expanded = |s: &str| {
            let expanded = expand_tilde_with_home(Path::new(s), home).into_inner();
            assert_eq!(expanded, Path::new(s));
        };

        let tests_expanded = vec!["~", "~/test/", "~//test/"];
        let tests_not_expanded: &[&[&str]] = &[
            &["1~1", "~user/", ""],
            // windows special
            &[r"\\.\~", r"\\?\~\", r"\\.\UNC\~", r"\\~"],
        ];

        for test in tests_expanded {
            check_expanded(test)
        }
        for test in tests_not_expanded.into_iter().map(|s| *s).flatten() {
            check_not_expanded(test)
        }
    }

    #[test]
    #[cfg(windows)]
    fn test_windows_weirdo_volume_relative_path() {
        let cwd = Path::new(r"C:\cwd");

        assert_eq!(
            relative_join(cwd, Path::new(r"C:data")),
            Path::new(r"C:\cwd\data")
        );
        assert_eq!(
            relative_join(cwd, Path::new(r"D:data")),
            Path::new(r"D:\data")
        );
    }
}
