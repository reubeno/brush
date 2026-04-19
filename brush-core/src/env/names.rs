//! Variable name types for the shell environment.
//!
//! The central type is [`VarName`], which encodes *how* a variable name should
//! be resolved when passed to [`ShellEnvironment`](super::ShellEnvironment)
//! methods:
//!
//! - [`VarName::Auto`] — resolve nameref chains transparently (default for `&str`)
//! - [`VarName::Resolved`] — already resolved; look up by exact base name
//! - [`VarName::Direct`] — bypass nameref resolution; inspect the variable itself
//!
//! # Examples
//!
//! ```ignore
//! // Auto-resolve (the default when passing &str):
//! env.get("ref")                           // follows ref → target
//! env.update_or_add("ref", value, ...)     // writes to target
//!
//! // Pre-resolved (from a prior resolve_nameref call):
//! let resolved = env.resolve_nameref("ref")?;
//! env.update_or_add(resolved, value, ...)  // skips re-resolution
//!
//! // Direct (bypass namerefs):
//! env.update_or_add(VarName::direct("ref"), value, ...)  // writes to ref itself
//! env.unset(VarName::direct("ref"))                       // removes ref itself
//! ```

/// How to resolve a variable name for environment operations.
///
/// Each variant encodes a resolution strategy that [`ShellEnvironment`](super::ShellEnvironment)
/// methods use to decide whether to follow nameref chains, use a pre-resolved result,
/// or look up the variable directly.
///
/// Construct using [`VarName::direct`] for bypass mode, or pass a `&str`/`String`/`ResolvedName`
/// which converts to [`VarName::Auto`]/[`VarName::Resolved`] via the `From` impls.
///
/// For a more ergonomic fluent style, use the [`VarNameExt`] trait:
/// ```ignore
/// use brush_core::env::VarNameExt;
/// env.unset("ref".direct())
/// ```
#[derive(Clone, Debug)]
pub enum VarName {
    /// Follow nameref chains transparently.
    ///
    /// This is the default when passing a bare `&str` or `String`.
    Auto(String),

    /// Already resolved through the nameref chain by a prior call to
    /// [`ShellEnvironment::resolve_nameref`](super::ShellEnvironment::resolve_nameref).
    ///
    /// The environment will look up `base` by exact name and attach the optional
    /// `subscript` for subscript-aware value extraction.
    Resolved {
        /// The base variable name (after nameref resolution and subscript extraction).
        base: String,
        /// The array subscript, if the resolved target includes one.
        subscript: Option<String>,
    },

    /// Look up the variable directly, bypassing nameref resolution.
    ///
    /// Use this when you want to inspect or modify the variable *itself* — e.g.,
    /// checking if it is a nameref (`[[ -R ref ]]`), removing it with `unset -n`,
    /// or writing to a `for`-in loop control variable.
    Direct(String),
}

impl VarName {
    /// Convenience constructor for [`VarName::Direct`].
    pub fn direct(name: impl Into<String>) -> Self {
        Self::Direct(name.into())
    }

    /// Returns the base name for a direct `HashMap` lookup, regardless of variant.
    ///
    /// For `Auto`, returns the raw name (caller must resolve first).
    /// For `Resolved`, returns the base name.
    /// For `Direct`, returns the name as-is.
    pub(crate) fn as_lookup_key(&self) -> &str {
        match self {
            Self::Auto(s) | Self::Direct(s) => s,
            Self::Resolved { base, .. } => base,
        }
    }

    /// Returns the subscript, if present in a `Resolved` variant.
    #[expect(dead_code)]
    pub(crate) fn subscript(&self) -> Option<&str> {
        match self {
            Self::Resolved { subscript, .. } => subscript.as_deref(),
            _ => None,
        }
    }

    /// Returns `true` if this is a `Resolved` variant with a subscript.
    #[expect(dead_code)]
    pub(crate) const fn has_subscript(&self) -> bool {
        matches!(
            self,
            Self::Resolved {
                subscript: Some(_),
                ..
            }
        )
    }
}

impl From<String> for VarName {
    fn from(s: String) -> Self {
        Self::Auto(s)
    }
}

impl From<&str> for VarName {
    fn from(s: &str) -> Self {
        Self::Auto(s.to_owned())
    }
}

impl From<&String> for VarName {
    fn from(s: &String) -> Self {
        Self::Auto(s.clone())
    }
}

impl From<super::ResolvedName> for VarName {
    fn from(r: super::ResolvedName) -> Self {
        Self::Resolved {
            base: r.name,
            subscript: r.subscript,
        }
    }
}

impl From<&super::ResolvedName> for VarName {
    fn from(r: &super::ResolvedName) -> Self {
        Self::Resolved {
            base: r.name.clone(),
            subscript: r.subscript.clone(),
        }
    }
}

/// A fully resolved nameref target, split into base name and optional array subscript.
///
/// When a nameref resolves to a plain variable name like `"target"`, `subscript` is `None`.
/// When it resolves to an array element like `"arr[2]"`, `name` is `"arr"` and `subscript`
/// is `Some("2")`.
///
/// Constructed by [`ShellEnvironment::resolve_nameref`](super::ShellEnvironment::resolve_nameref).
/// Converts to [`VarName::Resolved`] via the `From` impl, so you can pass it directly
/// to methods that accept `impl Into<VarName>`.
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

    /// Creates a `ResolvedName` wrapping a name that the caller asserts has
    /// **already been resolved** through the nameref chain.
    ///
    /// Prefer converting to [`VarName`] via the `From` impl instead of using
    /// this directly in new code.
    pub fn already_resolved(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
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
pub(crate) fn parse_nameref_subscript(target: &str) -> (&str, Option<&str>) {
    let Some(without_bracket) = target.strip_suffix(']') else {
        return (target, None);
    };
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

/// Extension trait for ergonomic `VarName::Direct` construction.
///
/// Instead of `VarName::direct("name")`, write `"name".direct()`.
///
/// ```ignore
/// use brush_core::env::VarNameExt;
/// env.unset("ref".direct())
/// ```
pub trait VarNameExt: Sized {
    /// Construct a [`VarName::Direct`] that bypasses nameref resolution.
    fn direct(self) -> VarName;
}

impl VarNameExt for &str {
    fn direct(self) -> VarName {
        VarName::Direct(self.to_owned())
    }
}

impl VarNameExt for String {
    fn direct(self) -> VarName {
        VarName::Direct(self)
    }
}

impl VarNameExt for &String {
    fn direct(self) -> VarName {
        VarName::Direct(self.clone())
    }
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

    #[test]
    fn resolved_name_from_name_no_subscript() {
        let r = ResolvedName::already_resolved("target");
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
        assert_eq!(r.subscript(), Some("2"));
    }

    #[test]
    fn resolved_name_into_name_consumes() {
        let r = ResolvedName::parse("arr[k]".to_owned());
        assert_eq!(r.into_name(), "arr");
    }

    #[test]
    fn varname_from_str_is_auto() {
        let vn: VarName = "foo".into();
        assert!(matches!(vn, VarName::Auto(s) if s == "foo"));
    }

    #[test]
    fn varname_from_string_is_auto() {
        let vn: VarName = String::from("bar").into();
        assert!(matches!(vn, VarName::Auto(s) if s == "bar"));
    }

    #[test]
    fn varname_from_ref_string_is_auto() {
        let s = String::from("baz");
        let vn: VarName = (&s).into();
        assert!(matches!(vn, VarName::Auto(t) if t == "baz"));
    }

    #[test]
    fn varname_from_resolved_name() {
        let r = ResolvedName::parse("arr[2]".to_owned());
        let vn: VarName = r.into();
        assert!(
            matches!(vn, VarName::Resolved { base, subscript } if base == "arr" && subscript == Some("2".to_owned()))
        );
    }

    #[test]
    fn varname_direct_constructor() {
        let vn = VarName::direct("ref");
        assert!(matches!(vn, VarName::Direct(s) if s == "ref"));
    }

    #[test]
    fn varname_as_lookup_key() {
        assert_eq!(VarName::Auto("x".into()).as_lookup_key(), "x");
        assert_eq!(VarName::Direct("y".into()).as_lookup_key(), "y");
        assert_eq!(
            VarName::Resolved {
                base: "z".into(),
                subscript: None
            }
            .as_lookup_key(),
            "z"
        );
    }

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
}
