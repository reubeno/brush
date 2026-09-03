//! Shared command-name resolution, used by the `type` and `command` builtins.

use std::io::Write;
use std::path::{Path, PathBuf};

use brush_core::{
    Shell, ShellExtensions,
    parser::ast,
    sys::{self, fs::PathExt},
};

/// A way in which a name resolved, in the shell's lookup order.
pub(crate) enum Resolved<'a> {
    /// An alias, with its target.
    Alias(String),
    /// A shell keyword.
    Keyword,
    /// A shell function, with its definition.
    Function(&'a ast::FunctionDefinition),
    /// A built-in command.
    Builtin,
    /// An executable file; `hashed` indicates it came from the program location cache.
    File { path: PathBuf, hashed: bool },
}

/// Options for [`resolve`]; the defaults match a plain `type NAME` lookup.
#[derive(Default)]
pub(crate) struct Options {
    /// Only search the filesystem, even if the name is an alias, keyword, function, or builtin.
    pub force_path_search: bool,
    /// Don't consider functions when resolving the name.
    pub suppress_func_lookup: bool,
    /// Report every location the name resolves to, not just the first.
    pub all_locations: bool,
}

/// Resolves the given name, returning the ways it resolved (in lookup order). Unless
/// `all_locations` was requested, at most one way is returned.
pub(crate) fn resolve<'a, SE: ShellExtensions>(
    shell: &'a Shell<SE>,
    name: &str,
    options: &Options,
) -> Vec<Resolved<'a>> {
    let mut resolved = vec![];

    // These are all hash lookups, so there's nothing to save by stopping at the first hit;
    // any extras are trimmed off at the end.
    if !options.force_path_search {
        // Check for aliases.
        if let Some(target) = shell.aliases().get(name) {
            resolved.push(Resolved::Alias(target.clone()));
        }

        // Check for keywords.
        if shell.is_keyword(name) {
            resolved.push(Resolved::Keyword);
        }

        // Check for functions.
        if !options.suppress_func_lookup
            && let Some(registration) = shell.funcs().get(name)
        {
            resolved.push(Resolved::Function(registration.definition()));
        }

        // Check for builtins.
        if shell.builtins().get(name).is_some_and(|b| !b.disabled) {
            resolved.push(Resolved::Builtin);
        }
    }

    // Searching the filesystem *does* cost something, so only do it if the results so far
    // don't already answer the question.
    if options.all_locations || resolved.is_empty() {
        resolve_in_filesystem(shell, name, options, &mut resolved);
    }

    if !options.all_locations {
        resolved.truncate(1);
    }

    resolved
}

/// Appends the files the given name resolves to.
fn resolve_in_filesystem<SE: ShellExtensions>(
    shell: &Shell<SE>,
    name: &str,
    options: &Options,
    resolved: &mut Vec<Resolved<'_>>,
) {
    let to_file = |path| Resolved::File {
        path,
        hashed: false,
    };

    // A name with a separator in it is used as-is; it's never searched for.
    if sys::fs::contains_path_separator(name) {
        if shell.absolute_path(Path::new(name)).executable() {
            resolved.push(to_file(PathBuf::from(name)));
        }
        return;
    }

    if let Some(path) = shell.program_location_cache().get(name) {
        resolved.push(Resolved::File { path, hashed: true });
        if !options.all_locations {
            return;
        }
    }

    resolved.extend(
        shell
            .find_executables_in_path(name)
            .take(if options.all_locations { usize::MAX } else { 1 })
            .map(to_file),
    );
}

/// Writes the description shown by `type NAME` and `command -V NAME`, newline included.
pub(crate) fn describe(
    mut writer: impl Write,
    name: &str,
    resolved: &Resolved<'_>,
) -> std::io::Result<()> {
    match resolved {
        Resolved::Alias(target) => writeln!(writer, "{name} is aliased to `{target}'"),
        Resolved::Keyword => writeln!(writer, "{name} is a shell keyword"),
        Resolved::Function(def) => writeln!(writer, "{name} is a function\n{def}"),
        Resolved::Builtin => writeln!(writer, "{name} is a shell builtin"),
        Resolved::File { path, hashed } => {
            let path = path.to_string_lossy();
            if *hashed {
                writeln!(writer, "{name} is hashed ({path})")
            } else {
                writeln!(writer, "{name} is {path}")
            }
        }
    }
}
