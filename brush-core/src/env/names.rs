//! Variable name types for the shell environment.
//!
//! The central types are [`ResolvedName`] (a nameref-resolved variable target)
//! and [`NameRef`] (the resolution strategy for mutations).

/// How a variable name should be resolved for a mutation (unset, update, etc.).
///
/// | Variant       | Follows chain | Target of write                    |
/// |---------------|---------------|------------------------------------|
/// | `Resolve`     | yes           | final target (base + any subscript)|
/// | `PreResolved` | already done  | pre-resolved target + subscript    |
/// | `Bypass`      | no            | the named variable itself          |
///
/// `Bypass` is a *semantic* choice (write the nameref var itself, e.g. for
/// `unset -n` or `for ref in …`), not a perf shortcut. Use `PreResolved` to
/// skip re-resolution while preserving subscripts.
#[derive(Clone, Debug)]
pub enum NameRef {
    /// Follow nameref chains before writing. Default; `&str` / `String` convert here.
    Resolve(String),
    /// Name already resolved via
    /// [`ShellEnvironment::resolve_nameref`](super::ShellEnvironment::resolve_nameref).
    PreResolved(ResolvedName),
    /// Write the named variable itself; no chain following, no subscript parsing.
    Bypass(String),
}

impl NameRef {
    /// Constructs a [`NameRef::Resolve`]. Equivalent to `name.into()`.
    pub fn resolve(name: impl Into<String>) -> Self {
        Self::Resolve(name.into())
    }

    /// Constructs a [`NameRef::PreResolved`]. Equivalent to `resolved.into()`.
    pub const fn pre_resolved(resolved: ResolvedName) -> Self {
        Self::PreResolved(resolved)
    }

    /// Constructs a [`NameRef::Bypass`]: write the named variable itself,
    /// not what it references. See the variant docs.
    ///
    /// Debug-asserts that `name` doesn't contain `[`. Reach for
    /// [`PreResolved`](Self::PreResolved) when you want to skip re-resolution
    /// while preserving subscripts.
    pub fn bypass(name: impl Into<String>) -> Self {
        let name = name.into();
        assert_bare_name(&name, "NameRef::bypass");
        Self::Bypass(name)
    }
}

impl From<String> for NameRef {
    fn from(s: String) -> Self {
        Self::Resolve(s)
    }
}

impl From<&str> for NameRef {
    fn from(s: &str) -> Self {
        Self::Resolve(s.to_owned())
    }
}

impl From<&String> for NameRef {
    fn from(s: &String) -> Self {
        Self::Resolve(s.clone())
    }
}

impl From<ResolvedName> for NameRef {
    fn from(r: ResolvedName) -> Self {
        Self::PreResolved(r)
    }
}

/// A fully resolved nameref target, split into base name and optional array subscript.
///
/// When a nameref resolves to a plain variable name like `"target"`, `subscript` is `None`.
/// When it resolves to an array element like `"arr[2]"`, `name` is `"arr"` and `subscript`
/// is `Some("2")`.
///
/// Constructed by [`ShellEnvironment::resolve_nameref`](super::ShellEnvironment::resolve_nameref).
/// Converts to [`NameRef::PreResolved`] via the `From` impl, so you can pass it directly
/// to methods that accept `impl Into<NameRef>`.
///
/// To look up a variable without nameref resolution, use
/// `env.lookup("name").bypassing_nameref().get()`.
#[derive(Clone, Debug)]
pub struct ResolvedName {
    pub(super) name: String,
    pub(super) subscript: Option<String>,
}

impl ResolvedName {
    /// The base variable name (after nameref resolution and subscript extraction).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The array subscript, if the resolved target includes one (e.g., `arr[2]` yields `Some("2")`).
    pub fn subscript(&self) -> Option<&str> {
        self.subscript.as_deref()
    }

    /// Consumes this `ResolvedName` and returns the base variable name.
    pub fn into_name(self) -> String {
        self.name
    }

    /// Returns a copy with the subscript stripped, keeping only the base name.
    ///
    /// Useful when a nameref resolved to a subscripted target (e.g., `arr[2]`)
    /// but you need to operate on the base variable (e.g., for attribute changes
    /// or whole-array expansion).
    #[must_use]
    pub fn without_subscript(&self) -> Self {
        Self {
            name: self.name.clone(),
            subscript: None,
        }
    }

    /// Parse a resolved nameref target string into base name and optional subscript.
    pub(super) fn parse(resolved: String) -> Self {
        let (base, sub) = parse_nameref_subscript(&resolved);
        if let Some(idx) = sub {
            Self {
                name: base.to_owned(),
                subscript: Some(idx.to_owned()),
            }
        } else {
            Self {
                name: resolved,
                subscript: None,
            }
        }
    }

    /// Wraps a plain (non-subscripted) name as a `ResolvedName`. Use when
    /// you've resolved a name externally and want to pass it to
    /// `lookup_resolved` without re-walking the chain.
    ///
    /// Debug-asserts `name` doesn't contain `[` — for subscripted targets,
    /// use [`ResolvedName::parse`] (which splits `"arr[2]"` into base + subscript).
    pub fn plain(name: impl Into<String>) -> Self {
        let name = name.into();
        assert_bare_name(&name, "ResolvedName::plain");
        Self {
            name,
            subscript: None,
        }
    }
}

/// Parse a potential `name[index]` subscript from a resolved nameref target string.
/// Returns `(base_name, Some(index))` if a subscript is present, or `(original, None)`.
///
/// Splits on the first `[` and requires a trailing `]`. Everything between the first
/// `[` and the final `]` is the index, which may contain arbitrary characters (including
/// nested brackets) for associative array keys.
///
/// This is a pure parser — callers are responsible for validating that the returned
/// base name is a valid variable name (see [`valid_variable_name`]).
pub(crate) fn parse_nameref_subscript(target: &str) -> (&str, Option<&str>) {
    // The target must end with `]` for a subscript to be present.
    let Some(without_bracket) = target.strip_suffix(']') else {
        return (target, None);
    };
    // Split on the first `[`. Everything before it is the variable name;
    // everything after (up to the stripped `]`) is the index.
    if let Some((name, index)) = without_bracket.split_once('[') {
        if !name.is_empty() {
            return (name, Some(index));
        }
    }
    (target, None)
}

/// Returns `true` if `target` is a valid nameref target name: the base name
/// (before any `[subscript]`) must be a legal variable name.
///
/// Does NOT check for self-references — callers must handle that separately.
pub fn valid_nameref_target_name(target: &str) -> bool {
    let (base, _) = parse_nameref_subscript(target);
    valid_variable_name(base)
}

/// Checks if the given name is a valid variable name.
pub fn valid_variable_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        Some(_) | None => false,
    }
}

/// Debug-only check that `name` is a bare variable name (no `[subscript]`).
/// `context` is the API surface name used in the panic message — e.g.
/// `"NameRef::bypass"` or `"ShellEnvironment::add"`.
///
/// In release builds this compiles to nothing.
#[inline]
pub(crate) fn assert_bare_name(name: &str, context: &str) {
    debug_assert!(
        !name.contains('['),
        "{context}: bare variable name required (no subscript); got {name:?}",
    );
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_variable_name() {
        assert!(!valid_variable_name(""));
        assert!(!valid_variable_name("1"));
        assert!(!valid_variable_name(" a"));
        assert!(!valid_variable_name(" "));

        assert!(valid_variable_name("_"));
        assert!(valid_variable_name("_a"));
        assert!(valid_variable_name("_1"));
        assert!(valid_variable_name("_a1"));
        assert!(valid_variable_name("a"));
        assert!(valid_variable_name("A"));
        assert!(valid_variable_name("a1"));
        assert!(valid_variable_name("A1"));
    }

    //
    // parse_nameref_subscript
    //

    #[test]
    fn parse_nameref_subscript_no_subscript() {
        assert_eq!(parse_nameref_subscript("foo"), ("foo", None));
        assert_eq!(parse_nameref_subscript(""), ("", None));
    }

    #[test]
    fn parse_nameref_subscript_simple() {
        assert_eq!(parse_nameref_subscript("arr[2]"), ("arr", Some("2")));
        assert_eq!(parse_nameref_subscript("map[key]"), ("map", Some("key")));
    }

    #[test]
    fn parse_nameref_subscript_special_indices() {
        assert_eq!(parse_nameref_subscript("arr[@]"), ("arr", Some("@")));
        assert_eq!(parse_nameref_subscript("arr[*]"), ("arr", Some("*")));
        assert_eq!(parse_nameref_subscript("arr[-1]"), ("arr", Some("-1")));
    }

    #[test]
    fn parse_nameref_subscript_empty_brackets() {
        assert_eq!(parse_nameref_subscript("arr[]"), ("arr", Some("")));
    }

    #[test]
    fn parse_nameref_subscript_missing_open_bracket() {
        assert_eq!(parse_nameref_subscript("foo]"), ("foo]", None));
    }

    #[test]
    fn parse_nameref_subscript_missing_close_bracket() {
        assert_eq!(parse_nameref_subscript("arr[2"), ("arr[2", None));
    }

    #[test]
    fn parse_nameref_subscript_empty_name() {
        assert_eq!(parse_nameref_subscript("[idx]"), ("[idx]", None));
    }

    #[test]
    fn parse_nameref_subscript_nested_brackets() {
        assert_eq!(parse_nameref_subscript("arr[a[b]]"), ("arr", Some("a[b]")));
        assert_eq!(parse_nameref_subscript("arr[[x]]"), ("arr", Some("[x]")));
    }

    //
    // ResolvedName construction & accessors
    //

    #[test]
    fn resolved_name_from_name_no_subscript() {
        let r = ResolvedName::plain("target");
        assert_eq!(r.name(), "target");
        assert_eq!(r.subscript(), None);
    }

    #[test]
    fn resolved_name_parse_with_subscript() {
        let r = ResolvedName::parse("arr[5]".to_owned());
        assert_eq!(r.name(), "arr");
        assert_eq!(r.subscript(), Some("5"));
    }

    #[test]
    fn resolved_name_parse_without_subscript() {
        let r = ResolvedName::parse("plain".to_owned());
        assert_eq!(r.name(), "plain");
        assert_eq!(r.subscript(), None);
    }

    #[test]
    fn resolved_name_without_subscript_strips_index() {
        let r = ResolvedName::parse("arr[2]".to_owned());
        let stripped = r.without_subscript();
        assert_eq!(stripped.name(), "arr");
        assert_eq!(stripped.subscript(), None);
        // Original is unchanged.
        assert_eq!(r.subscript(), Some("2"));
    }

    #[test]
    fn resolved_name_into_name_consumes() {
        let r = ResolvedName::parse("arr[k]".to_owned());
        assert_eq!(r.into_name(), "arr");
    }

    //
    // valid_nameref_target_name
    //

    #[test]
    fn valid_nameref_target_simple() {
        assert!(valid_nameref_target_name("foo"));
        assert!(valid_nameref_target_name("_bar"));
        assert!(valid_nameref_target_name("arr[2]"));
        assert!(valid_nameref_target_name("arr[@]"));
    }

    #[test]
    fn invalid_nameref_target() {
        assert!(!valid_nameref_target_name(""));
        assert!(!valid_nameref_target_name("1bad"));
        assert!(!valid_nameref_target_name("[idx]"));
    }

    //
    // NameRef construction & From impls
    //

    #[test]
    fn nameref_from_str_is_resolve() {
        let nr: NameRef = "foo".into();
        assert!(matches!(nr, NameRef::Resolve(s) if s == "foo"));
    }

    #[test]
    fn nameref_from_string_is_resolve() {
        let nr: NameRef = String::from("bar").into();
        assert!(matches!(nr, NameRef::Resolve(s) if s == "bar"));
    }

    #[test]
    fn nameref_from_ref_string_is_resolve() {
        let s = String::from("baz");
        let nr: NameRef = (&s).into();
        assert!(matches!(nr, NameRef::Resolve(t) if t == "baz"));
    }

    #[test]
    fn nameref_from_resolved_name() {
        let r = ResolvedName::parse("arr[2]".to_owned());
        let nr: NameRef = r.into();
        match nr {
            NameRef::PreResolved(r) => {
                assert_eq!(r.name(), "arr");
                assert_eq!(r.subscript(), Some("2"));
            }
            other => panic!("expected PreResolved, got {other:?}"),
        }
    }

    #[test]
    fn nameref_bypass_constructor() {
        let nr = NameRef::bypass("ref");
        assert!(matches!(nr, NameRef::Bypass(s) if s == "ref"));
    }

    #[test]
    fn nameref_resolve_constructor() {
        let nr = NameRef::resolve("ref");
        assert!(matches!(nr, NameRef::Resolve(s) if s == "ref"));
    }

    #[test]
    fn nameref_pre_resolved_constructor() {
        let r = ResolvedName::plain("ref");
        let nr = NameRef::pre_resolved(r);
        assert!(matches!(nr, NameRef::PreResolved(r) if r.name() == "ref"));
    }
}
