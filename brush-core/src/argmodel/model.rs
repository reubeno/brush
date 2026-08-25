//! A backend-neutral description of built-in command arguments.
//!
//! Builtins describe their arguments once, as data ([`CommandSpec`]), and read
//! parsed values back out of a [`Matches`] view. Which argument-parsing crate
//! turns the spec into an actual parser is an implementation detail selected at
//! compile time (see [`backend`]).
//!
//! The model intentionally covers only what brush builtins need:
//!
//! * named switches (`bool` flags),
//! * named value-taking options (`String`/parsed values),
//! * positional operands, optionally repeating,
//! * verbatim trailing operands with knowledge of which short options take
//!   values (shell-style option-section termination),
//! * `+`-style option groups (`set +vx`),
//! * help metadata (`about`, synopsis).
//!
//! Anything richer than that stays in the builtin's own `execute`.

use std::collections::HashMap;

/// Kind of a named argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    /// A boolean switch.
    Flag,
    /// An option that takes one value.
    Value,
}

/// Description of a single named argument (switch or option).
#[derive(Clone)]
pub struct ArgSpec {
    /// Identifier used to look the value up in [`Matches`]; typically the
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

/// Description of a positional operand.
#[derive(Clone)]
pub struct PositionalSpec {
    /// Identifier used to look the value up in [`Matches`].
    pub id: &'static str,

    /// Metavariable name shown in usage/help.
    pub name: &'static str,

    /// Whether more than one value may be supplied.
    pub many: bool,

    /// Whether operands that look like flags are accepted. Shell builtins
    /// whose operands are interpreted entirely by `execute` need this.
    pub accepts_flag_like: bool,
}

/// A fully described built-in command's argument surface.
#[derive(Clone, Default)]
pub struct CommandSpec {
    /// Named arguments, in declaration order.
    pub args: Vec<ArgSpec>,

    /// Positional operands, in declaration order.
    pub positionals: Vec<PositionalSpec>,
}

impl CommandSpec {
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

/// Builder for [`CommandSpec`].
#[derive(Default)]
pub struct CommandSpecBuilder {
    spec: CommandSpec,
}

impl CommandSpecBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            spec: CommandSpec {
                args: Vec::new(),
                positionals: Vec::new(),
            },
        }
    }

    /// Adds a named argument.
    ///
    /// # Arguments
    ///
    /// * `id` - Lookup key in [`Matches`].
    /// * `shorts` - Short names (first is visible).
    /// * `longs` - Long names (first visible, rest hidden aliases).
    /// * `kind` - Switch or value-taking.
    /// * `metavar` - Metavariable for value-taking arguments.
    /// * `help` - One-line description.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn arg(
        mut self,
        id: &'static str,
        shorts: &'static [char],
        longs: &'static [&'static str],
        kind: ArgKind,
        metavar: Option<&'static str>,
        help: &'static str,
    ) -> Self {
        self.spec.args.push(ArgSpec {
            id,
            shorts,
            longs,
            hidden: false,
            kind,
            metavar,
            help,
        });
        self
    }

    /// Adds a hidden named argument (accepted but not shown in help).
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn hidden_arg(
        mut self,
        id: &'static str,
        shorts: &'static [char],
        longs: &'static [&'static str],
        kind: ArgKind,
        metavar: Option<&'static str>,
        help: &'static str,
    ) -> Self {
        self.spec.args.push(ArgSpec {
            id,
            shorts,
            longs,
            hidden: true,
            kind,
            metavar,
            help,
        });
        self
    }

    /// Adds a positional operand that accepts at most one value.
    #[must_use]
    pub fn positional(mut self, id: &'static str, name: &'static str) -> Self {
        self.spec.positionals.push(PositionalSpec {
            id,
            name,
            many: false,
            accepts_flag_like: false,
        });
        self
    }

    /// Adds a positional operand that accepts any number of values.
    #[must_use]
    pub fn positional_many(mut self, id: &'static str, name: &'static str) -> Self {
        self.spec.positionals.push(PositionalSpec {
            id,
            name,
            many: true,
            accepts_flag_like: false,
        });
        self
    }

    /// Finalizes the spec.
    #[must_use]
    pub fn build(self) -> CommandSpec {
        self.spec
    }
}

/// Parsed values for a [`CommandSpec`], produced by an argument backend.
///
/// Values are keyed by the `id`s used when declaring the spec. Trailing
/// verbatim operands (captured outside the parsed option section) live here
/// too, under their own key.
#[derive(Debug, Default)]
pub struct Matches {
    flags: HashMap<&'static str, bool>,
    values: HashMap<&'static str, Vec<String>>,
    trailing: Vec<String>,
}

impl Matches {
    /// Creates an empty matches container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a switch as present.
    pub fn set_flag(&mut self, id: &'static str) {
        self.flags.insert(id, true);
    }

    /// Records one occurrence of a value-taking option.
    pub fn push_value(&mut self, id: &'static str, value: String) {
        self.values.entry(id).or_default().push(value);
    }

    /// Replaces all recorded values for `id`.
    pub fn set_values(&mut self, id: &'static str, values: Vec<String>) {
        self.values.insert(id, values);
    }

    /// Replaces the captured trailing operands.
    pub fn set_trailing(&mut self, trailing: Vec<String>) {
        self.trailing = trailing;
    }

    /// Returns whether the switch `id` was present.
    pub fn flag(&self, id: &str) -> bool {
        self.flags.get(id).copied().unwrap_or(false)
    }

    /// Returns the last value recorded for `id`.
    pub fn value(&self, id: &str) -> Option<&str> {
        self.values
            .get(id)
            .and_then(|v| v.last().map(String::as_str))
    }

    /// Returns all values recorded for `id`, in order.
    pub fn values(&self, id: &str) -> &[String] {
        match self.values.get(id) {
            Some(values) => values.as_slice(),
            None => &[],
        }
    }

    /// Returns whether anything was recorded for `id`.
    pub fn has_value(&self, id: &str) -> bool {
        self.values.contains_key(id)
    }

    /// Returns the captured trailing operands.
    pub fn trailing(&self) -> &[String] {
        &self.trailing
    }
}

impl Matches {
    /// Stores trailing operands under the reserved `"trailing"` key.
    pub(crate) fn set_trailing_args_placeholder(&mut self, trailing: Vec<String>) {
        self.set_trailing(trailing);
    }
}
