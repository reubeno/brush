//! Variable name types for the shell environment.
//!
//! The central types are [`ResolvedName`] (a nameref-resolved variable target),
//! [`NameRef`] (the resolution strategy for mutations), and [`NameRefFault`]
//! (a resolution failure callers must explicitly recover from).

/// A failure to resolve a nameref chain: either a true cycle or a chain that
/// exceeds the maximum supported resolution depth.
///
/// Returned as the `Err` arm of
/// [`resolve_nameref`](super::ShellEnvironment::resolve_nameref) and friends so
/// that a caller resolving directly must `match` the fault and choose a recovery
/// policy (warn+skip, warn+identity, silent identity, or propagate), rather than
/// rediscovering it from an `Error::kind()` match.
///
/// The convenience mutators that resolve internally —
/// [`set_var`](super::ShellEnvironment::set_var),
/// [`update_or_add`](super::ShellEnvironment::update_or_add), and
/// [`unset`](super::ShellEnvironment::unset) — cannot return this type, so they
/// surface a fault as [`ErrorKind::NameRef`](crate::error::ErrorKind::NameRef)
/// via the `From<NameRefFault>` conversion (i.e. `?` propagates it as the shell
/// `Error`). Callers that need to *handle* the fault (warn-and-skip, etc.)
/// should resolve up front with `resolve_nameref` instead of relying on the
/// mutator's propagated `Error`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRefFault {
    kind: NameRefFaultKind,
    head: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NameRefFaultKind {
    /// The chain forms a cycle (a variable transitively references itself).
    Circular,
    /// The chain exceeded the maximum resolution depth without terminating.
    MaxDepthExceeded { max: usize },
}

impl NameRefFault {
    /// Constructs a circular-reference fault blaming `head` (the variable that
    /// resolution started from — the name bash names in its diagnostic).
    pub(crate) fn circular(head: impl Into<String>) -> Self {
        Self {
            kind: NameRefFaultKind::Circular,
            head: head.into(),
        }
    }

    /// Constructs a max-depth fault blaming `head`.
    pub(crate) fn max_depth(head: impl Into<String>, max: usize) -> Self {
        Self {
            kind: NameRefFaultKind::MaxDepthExceeded { max },
            head: head.into(),
        }
    }

    /// The variable name resolution started from — the one bash names in its
    /// diagnostic.
    pub fn head(&self) -> &str {
        &self.head
    }

    /// Returns `true` if this is a true cycle (as opposed to a too-deep but
    /// acyclic chain).
    pub const fn is_circular(&self) -> bool {
        matches!(self.kind, NameRefFaultKind::Circular)
    }
}

impl std::fmt::Display for NameRefFault {
    /// Renders the bash-compatible diagnostic text (without any `warning:`
    /// prefix), e.g. `"ref: circular name reference"` or
    /// `"ref: maximum nameref depth (8) exceeded"`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            NameRefFaultKind::Circular => {
                write!(f, "{}: circular name reference", self.head)
            }
            NameRefFaultKind::MaxDepthExceeded { max } => {
                write!(f, "{}: maximum nameref depth ({max}) exceeded", self.head)
            }
        }
    }
}

impl std::error::Error for NameRefFault {}

/// How a variable name should be resolved for a mutation (unset, update, etc.).
///
/// Construct via [`NameRef::resolve`] (follow chains; the `From<&str>`/`String`
/// default), [`NameRef::pre_resolved`] (target already resolved, subscript
/// preserved), or [`NameRef::bypass`] (write the named variable itself — a
/// *semantic* choice for `unset -n` / `for ref in …`, not a perf shortcut).
///
/// The discriminant is intentionally opaque: callers cannot construct a
/// `Bypass` carrying a subscripted name (which would silently no-op against a
/// `HashMap` keyed by the literal `"arr[2]"`), because the only construction
/// paths run through the validating constructors above.
#[derive(Clone, Debug)]
pub struct NameRef(pub(super) NameRefStrategy);

/// Internal discriminant for [`NameRef`]. Not part of the public API — see the
/// `NameRef` constructors. Matched only within the `env` module.
#[derive(Clone, Debug)]
pub(super) enum NameRefStrategy {
    /// Follow nameref chains before writing.
    Resolve(String),
    /// Name already resolved via
    /// [`ShellEnvironment::resolve_nameref`](super::ShellEnvironment::resolve_nameref).
    PreResolved(ResolvedName),
    /// Write the named variable itself; no chain following, no subscript parsing.
    Bypass(String),
}

impl NameRef {
    /// Constructs a chain-following reference. Equivalent to `name.into()`.
    pub fn resolve(name: impl Into<String>) -> Self {
        Self(NameRefStrategy::Resolve(name.into()))
    }

    /// Constructs a pre-resolved reference. Equivalent to `resolved.into()`.
    pub const fn pre_resolved(resolved: ResolvedName) -> Self {
        Self(NameRefStrategy::PreResolved(resolved))
    }

    /// Constructs a bypass reference: write the named variable itself, not what
    /// it references. See the type docs.
    ///
    /// Debug-asserts that `name` doesn't contain `[`. Reach for
    /// [`pre_resolved`](Self::pre_resolved) when you want to skip re-resolution
    /// while preserving subscripts.
    pub fn bypass(name: impl Into<String>) -> Self {
        let name = name.into();
        assert_bare_name(&name, "NameRef::bypass");
        Self(NameRefStrategy::Bypass(name))
    }
}

impl From<String> for NameRef {
    fn from(s: String) -> Self {
        Self::resolve(s)
    }
}

impl From<&str> for NameRef {
    fn from(s: &str) -> Self {
        Self::resolve(s)
    }
}

impl From<&String> for NameRef {
    fn from(s: &String) -> Self {
        Self::resolve(s.clone())
    }
}

impl From<ResolvedName> for NameRef {
    fn from(r: ResolvedName) -> Self {
        Self::pre_resolved(r)
    }
}

/// A fully resolved nameref target, split into base name and optional array subscript.
///
/// When a nameref resolves to a plain variable name like `"target"`, `subscript` is `None`.
/// When it resolves to an array element like `"arr[2]"`, `name` is `"arr"` and `subscript`
/// is `Some("2")`.
///
/// Constructed by [`ShellEnvironment::resolve_nameref`](super::ShellEnvironment::resolve_nameref).
/// Converts to a pre-resolved [`NameRef`] via the `From` impl (see
/// [`NameRef::pre_resolved`]), so you can pass it directly to methods that
/// accept `impl Into<NameRef>`.
///
/// To look up a variable without nameref resolution, use
/// `env.lookup("name").bypassing_nameref().get()`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
    /// Debug-asserts `name` doesn't contain `[` — subscripted targets must be
    /// produced by resolution (which splits `"arr[2]"` into base + subscript),
    /// not wrapped verbatim here.
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

/// Returns `true` if `target` is a valid nameref target name: either a bare
/// legal variable name, or a well-formed array reference `name[subscript]`.
///
/// "Well-formed" matches bash's `valid_array_reference`: the base before the
/// first `[` must be a legal variable name, and that `[`'s *matching* `]` (with
/// balanced nesting, so associative keys like `a[b]` are allowed) must be the
/// final character. This rejects malformed targets such as `m[a]b]`, `m[x][y]`,
/// and `a[b[c]` that a looser first-`[`/last-`]` split would otherwise accept as
/// a plausible-looking base + subscript.
///
/// Does NOT check for self-references — callers must handle that separately.
pub fn valid_nameref_target_name(target: &str) -> bool {
    // Split at the first `[`. No `[` means a bare name, which must be legal.
    let Some((base, rest)) = target.split_once('[') else {
        return valid_variable_name(target);
    };

    // The base before the first `[` must be a legal variable name.
    if !valid_variable_name(base) {
        return false;
    }

    // Walk `rest` (everything after the first `[`), tracking bracket depth. The
    // matching `]` (depth back to zero) must be the last character of `target`.
    let mut depth = 1usize;
    for (i, c) in rest.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    // `]` is ASCII (1 byte), so it's the last char iff its byte
                    // offset is the final index of `rest`.
                    return i + 1 == rest.len();
                }
            }
            _ => {}
        }
    }
    // No matching `]`.
    false
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
        assert!(valid_nameref_target_name("arr[]"));
        // Nested brackets (associative key) are allowed when balanced.
        assert!(valid_nameref_target_name("m[a[b]]"));
        assert!(valid_nameref_target_name("m[a[b]c]"));
    }

    #[test]
    fn invalid_nameref_target() {
        assert!(!valid_nameref_target_name(""));
        assert!(!valid_nameref_target_name("1bad"));
        assert!(!valid_nameref_target_name("[idx]"));
        // Malformed array references: the first `[`'s matching `]` must be the
        // final character (bash's valid_array_reference).
        assert!(!valid_nameref_target_name("m[a]b]"));
        assert!(!valid_nameref_target_name("m[x][y]"));
        assert!(!valid_nameref_target_name("a[b[c]"));
        assert!(!valid_nameref_target_name("arr[2]x"));
        assert!(!valid_nameref_target_name("arr["));
    }

    //
    // NameRef construction & From impls
    //

    #[test]
    fn nameref_from_str_is_resolve() {
        let nr: NameRef = "foo".into();
        assert!(matches!(nr.0, NameRefStrategy::Resolve(s) if s == "foo"));
    }

    #[test]
    fn nameref_from_string_is_resolve() {
        let nr: NameRef = String::from("bar").into();
        assert!(matches!(nr.0, NameRefStrategy::Resolve(s) if s == "bar"));
    }

    #[test]
    fn nameref_from_ref_string_is_resolve() {
        let s = String::from("baz");
        let nr: NameRef = (&s).into();
        assert!(matches!(nr.0, NameRefStrategy::Resolve(t) if t == "baz"));
    }

    #[test]
    fn nameref_from_resolved_name() {
        let r = ResolvedName::parse("arr[2]".to_owned());
        let nr: NameRef = r.into();
        match nr.0 {
            NameRefStrategy::PreResolved(r) => {
                assert_eq!(r.name(), "arr");
                assert_eq!(r.subscript(), Some("2"));
            }
            other => panic!("expected PreResolved, got {other:?}"),
        }
    }

    #[test]
    fn nameref_bypass_constructor() {
        let nr = NameRef::bypass("ref");
        assert!(matches!(nr.0, NameRefStrategy::Bypass(s) if s == "ref"));
    }

    #[test]
    fn nameref_resolve_constructor() {
        let nr = NameRef::resolve("ref");
        assert!(matches!(nr.0, NameRefStrategy::Resolve(s) if s == "ref"));
    }

    #[test]
    fn nameref_pre_resolved_constructor() {
        let r = ResolvedName::plain("ref");
        let nr = NameRef::pre_resolved(r);
        assert!(matches!(nr.0, NameRefStrategy::PreResolved(r) if r.name() == "ref"));
    }

    //
    // NameRefFault
    //

    #[test]
    fn nameref_fault_circular_message() {
        let f = NameRefFault::circular("ref");
        assert!(f.is_circular());
        assert_eq!(f.head(), "ref");
        assert_eq!(f.to_string(), "ref: circular name reference");
    }

    #[test]
    fn nameref_fault_max_depth_message() {
        let f = NameRefFault::max_depth("c1", 8);
        assert!(!f.is_circular());
        assert_eq!(f.head(), "c1");
        assert_eq!(f.to_string(), "c1: maximum nameref depth (8) exceeded");
    }
}
