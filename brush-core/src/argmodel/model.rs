//! A backend-neutral description of built-in command arguments.
//!
//! Specs are **compile-time data**: every field is `'static`, so each builtin
//! declares one `static CommandSpec` and hands out a reference to it. Building
//! a spec never allocates and never runs per parse; argument-parsing backends
//! receive `&'static CommandSpec` and may memoize whatever they derive from it.
//!
//! The model intentionally covers only what brush builtins need:
//!
//! * named switches (`bool` flags),
//! * named value-taking options,
//! * positional operands, optionally repeating or accepting flag-like words,
//! * verbatim trailing operands (split in core, before the backend runs),
//! * help metadata (`help` text, hidden-ness).
//!
//! Anything richer than that stays in the builtin's own `execute`.

/// Kind of a named argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    /// A boolean switch.
    Flag,
    /// An option that takes one value.
    Value,
}

/// Description of a single named argument (switch or option).
#[derive(Clone, Copy, Debug)]
pub struct ArgSpec {
    /// Identifier used to look the value up in [`ParsedValues`]; typically the
    /// destination field name.
    pub id: &'static str,

    /// Short names; the first one is shown in help output.
    pub shorts: &'static [char],

    /// Long names; the first one is shown in help output and the rest are
    /// hidden aliases.
    pub longs: &'static [&'static str],

    /// Whether this argument is hidden from help output entirely.
    pub hidden: bool,

    /// What the argument consumes.
    pub kind: ArgKind,

    /// Metavariable name for value-taking arguments (e.g., `"FILE"`).
    pub metavar: Option<&'static str>,

    /// One-line help text.
    pub help: &'static str,
}

impl ArgSpec {
    /// Declares a boolean switch.
    #[must_use]
    pub const fn flag(
        id: &'static str,
        shorts: &'static [char],
        longs: &'static [&'static str],
        help: &'static str,
    ) -> Self {
        Self {
            id,
            shorts,
            longs,
            hidden: false,
            kind: ArgKind::Flag,
            metavar: None,
            help,
        }
    }

    /// Declares a boolean switch hidden from help output.
    #[must_use]
    pub const fn hidden_flag(
        id: &'static str,
        shorts: &'static [char],
        longs: &'static [&'static str],
        help: &'static str,
    ) -> Self {
        Self {
            id,
            shorts,
            longs,
            hidden: true,
            kind: ArgKind::Flag,
            metavar: None,
            help,
        }
    }

    /// Declares an option that takes one value, hidden from help output.
    #[must_use]
    pub const fn hidden_value(
        id: &'static str,
        shorts: &'static [char],
        longs: &'static [&'static str],
        metavar: &'static str,
        help: &'static str,
    ) -> Self {
        Self {
            id,
            shorts,
            longs,
            hidden: true,
            kind: ArgKind::Value,
            metavar: Some(metavar),
            help,
        }
    }

    /// Declares an option that takes one value.
    #[must_use]
    pub const fn value(
        id: &'static str,
        shorts: &'static [char],
        longs: &'static [&'static str],
        metavar: &'static str,
        help: &'static str,
    ) -> Self {
        Self {
            id,
            shorts,
            longs,
            hidden: false,
            kind: ArgKind::Value,
            metavar: Some(metavar),
            help,
        }
    }
}

/// Description of a positional operand.
#[derive(Clone, Copy, Debug)]
pub struct PositionalSpec {
    /// Identifier used to look values up in [`ParsedValues`].
    pub id: &'static str,

    /// Metavariable name shown in usage/help.
    pub name: &'static str,

    /// Whether more than one value may be supplied.
    pub many: bool,

    /// Whether operands that look like flags are accepted. Shell builtins
    /// whose operands are interpreted entirely by `execute` need this;
    /// strict positionals reject flag-like words instead.
    pub accepts_flag_like: bool,
}

impl PositionalSpec {
    /// Declares a positional operand that accepts at most one value.
    #[must_use]
    pub const fn one(id: &'static str, name: &'static str) -> Self {
        Self {
            id,
            name,
            many: false,
            accepts_flag_like: false,
        }
    }

    /// Declares a positional operand that accepts any number of values.
    #[must_use]
    pub const fn many(id: &'static str, name: &'static str) -> Self {
        Self {
            id,
            name,
            many: true,
            accepts_flag_like: false,
        }
    }

    /// Declares a repeating positional that also accepts flag-like words.
    #[must_use]
    pub const fn verbatim(id: &'static str, name: &'static str) -> Self {
        Self {
            id,
            name,
            many: true,
            accepts_flag_like: true,
        }
    }

    /// Declares a single positional that also accepts flag-like words.
    #[must_use]
    pub const fn one_verbatim(id: &'static str, name: &'static str) -> Self {
        Self {
            id,
            name,
            many: false,
            accepts_flag_like: true,
        }
    }
}

/// A fully described built-in command's argument surface.
///
/// Constructible in `const` contexts; typical usage is one `static SPEC` per
/// builtin returned from [`crate::builtins::SpecCommand::spec`].
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    /// Named arguments, in declaration order.
    pub args: &'static [ArgSpec],

    /// Positional operands, in declaration order.
    pub positionals: &'static [PositionalSpec],
}

impl CommandSpec {
    /// An empty spec (commands that take no options at all).
    pub const EMPTY: Self = Self {
        args: &[],
        positionals: &[],
    };

    /// Returns the named argument registered under `id`, if any.
    #[must_use]
    pub fn arg(&self, id: &str) -> Option<&ArgSpec> {
        self.args.iter().find(|a| a.id == id)
    }

    /// Returns the positional registered under `id`, if any.
    #[must_use]
    pub fn positional(&self, id: &str) -> Option<&PositionalSpec> {
        self.positionals.iter().find(|p| p.id == id)
    }
}

/// Parsed values for a [`CommandSpec`], produced by an argument backend.
///
/// Storage is slot-indexed parallel to the spec's declaration arrays; lookups
/// by id are linear scans over those small static arrays (no hashing, no
/// allocation).
#[derive(Clone, Debug)]
pub struct ParsedValues {
    spec: &'static CommandSpec,
    flags: Vec<bool>,
    values: Vec<Vec<String>>,
    positionals: Vec<Vec<String>>,
    trailing: Vec<String>,
}

impl ParsedValues {
    /// Creates an empty container for the given spec.
    #[must_use]
    pub fn new(spec: &'static CommandSpec) -> Self {
        Self {
            spec,
            flags: vec![false; spec.args.len()],
            values: vec![Vec::new(); spec.args.len()],
            positionals: vec![Vec::new(); spec.positionals.len()],
            trailing: Vec::new(),
        }
    }

    /// The spec these values were parsed against.
    #[must_use]
    pub const fn spec(&self) -> &'static CommandSpec {
        self.spec
    }

    fn slot(&self, id: &str) -> Option<usize> {
        self.spec.args.iter().position(|a| a.id == id)
    }

    /// Records a switch as present.
    pub fn set_flag(&mut self, id: &'static str) {
        if let Some(ix) = self.slot(id) {
            self.flags[ix] = true;
        }
    }

    /// Records a switch as present directly at a resolved slot
    /// (backend-internal).
    pub fn set_flag_at(&mut self, slot: usize) {
        if let Some(flag) = self.flags.get_mut(slot) {
            *flag = true;
        }
    }

    /// Records one occurrence of a value-taking option.
    pub fn push_value(&mut self, id: &'static str, value: String) {
        if let Some(ix) = self.slot(id) {
            self.values[ix].push(value);
        }
    }

    /// Pushes a value directly into a resolved slot (backend-internal).
    pub fn push_value_at(&mut self, slot: usize, value: String) {
        if let Some(target) = self.values.get_mut(slot) {
            target.push(value);
        }
    }

    /// Replaces values directly at a resolved slot (backend-internal).
    pub fn set_values_at(&mut self, slot: usize, values: Vec<String>) {
        if let Some(target) = self.values.get_mut(slot) {
            *target = values;
        }
    }

    /// Replaces all recorded values for `id`.
    pub fn set_values(&mut self, id: &'static str, values: Vec<String>) {
        if let Some(ix) = self.slot(id) {
            self.values[ix] = values;
        }
    }

    /// Records one value for the positional with the given id.
    pub fn push_positional_by_id(&mut self, id: &str, value: String) {
        if let Some(ix) = self.spec.positionals.iter().position(|p| p.id == id) {
            self.push_positional_at(ix, value);
        }
    }

    /// Records one value for the positional at `slot` (backend-internal).
    pub fn push_positional_at(&mut self, slot: usize, value: String) {
        if let Some(target) = self.positionals.get_mut(slot) {
            target.push(value);
        }
    }

    /// Replaces values for the positional at `slot` (backend-internal).
    pub fn set_positional_at(&mut self, slot: usize, values: Vec<String>) {
        if let Some(target) = self.positionals.get_mut(slot) {
            *target = values;
        }
    }

    /// Returns all values recorded for the positional `id`.
    #[must_use]
    pub fn positional_values(&self, id: &str) -> &[String] {
        match self
            .spec
            .positional(id)
            .and_then(|p| self.spec.positionals.iter().position(|sp| sp.id == p.id))
        {
            Some(ix) => self.positionals.get(ix).map_or(&[], Vec::as_slice),
            None => &[],
        }
    }

    /// Replaces the captured trailing operands.
    pub fn set_trailing(&mut self, trailing: Vec<String>) {
        self.trailing = trailing;
    }

    /// Alias used by the `SpecCommand` flow.
    pub fn set_trailing_args_placeholder(&mut self, trailing: Vec<String>) {
        self.set_trailing(trailing);
    }

    /// Returns whether the switch `id` was present.
    #[must_use]
    pub fn flag(&self, id: &str) -> bool {
        self.slot(id).is_some_and(|ix| self.flags[ix])
    }

    /// Returns the last value recorded for `id`.
    #[must_use]
    pub fn value(&self, id: &str) -> Option<&str> {
        let ix = self.slot(id)?;
        self.values.get(ix)?.last().map(String::as_str)
    }

    /// Returns all values recorded for `id`, in order.
    #[must_use]
    pub fn values(&self, id: &str) -> &[String] {
        match self.slot(id) {
            Some(ix) => &self.values[ix],
            None => &[],
        }
    }

    /// Returns the last value recorded for the positional `id`.
    #[must_use]
    pub fn value_of_positional(&self, id: &str) -> Option<&str> {
        self.positional_values(id).last().map(String::as_str)
    }

    /// Returns whether anything was recorded for `id`.
    #[must_use]
    pub fn has_value(&self, id: &str) -> bool {
        self.slot(id).is_some_and(|ix| !self.values[ix].is_empty())
    }

    /// Returns the captured trailing operands.
    #[must_use]
    pub fn trailing(&self) -> &[String] {
        &self.trailing
    }
}
