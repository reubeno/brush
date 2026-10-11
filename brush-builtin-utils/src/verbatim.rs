//! Binds builtins that do no argument parsing to the contracts in
//! [`brush_core::builtins`]: arguments are ignored or stored verbatim, and the
//! help text is fixed.
//!
//! This is a second adapter beside [`crate::clap_adapter`], needing no engine
//! and no dependencies; it shows that the contracts assume neither.

/// Declares a builtin whose arguments are not parsed and whose help is fixed
/// text.
///
/// # Forms
///
/// Exactly one of these forms is used per type. `synopsis` and `description`
/// are required and `help` is optional; any other option, or a missing
/// required one, is a compile error naming the accepted forms.
///
/// | Form | Arguments |
/// |---|---|
/// | `verbatim_builtin!(Type, synopsis = "...", description = "..." [, help = "..."])` | Ignored, as `:`, `true`, and `false` ignore theirs. Constructs `Type {}`. |
/// | `verbatim_builtin!(Type, args = field, synopsis = "...", description = "..." [, help = "..."])` | Stored untouched in `field` (a `Vec<CommandArg>`, which must be the type's only field), with [`FromArgs::TAKES_DECLARATIONS`](brush_core::builtins::FromArgs::TAKES_DECLARATIONS) set so assignments arrive as assignments. For builtins such as `builtin` that forward their arguments. |
///
/// Without `help`, the detailed help is composed from the synopsis and
/// description.
///
/// # Examples
///
/// ```
/// struct TrueCommand {}
///
/// brush_builtin_utils::verbatim_builtin!(
///     TrueCommand,
///     synopsis = "true",
///     description = "success",
///     help = "Returns a successful exit status.\n",
/// );
///
/// struct BuiltinCommand {
///     args: Vec<brush_core::CommandArg>,
/// }
///
/// brush_builtin_utils::verbatim_builtin!(
///     BuiltinCommand,
///     args = args,
///     synopsis = "builtin [shell-builtin [arg ...]]",
///     description = "Execute shell builtins",
/// );
/// ```
///
/// Misuse is rejected with a message naming the accepted forms:
///
/// ```compile_fail
/// # struct T {}
/// brush_builtin_utils::verbatim_builtin!(T, synopsis = "t");
/// ```
#[macro_export]
macro_rules! verbatim_builtin {
    ($t:ty, args = $field:ident, synopsis = $synopsis:expr, description = $description:expr $(, help = $help:expr)? $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            const TAKES_DECLARATIONS: bool = true;

            fn from_args(
                _name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                ::std::result::Result::Ok(Self { $field: args })
            }
        }

        $crate::__verbatim_builtin_help!($t, $synopsis, $description $(, $help)?);
    };

    ($t:ty, synopsis = $synopsis:expr, description = $description:expr $(, help = $help:expr)? $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            fn from_args(
                _name: &str,
                _args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                ::std::result::Result::Ok(Self {})
            }
        }

        $crate::__verbatim_builtin_help!($t, $synopsis, $description $(, $help)?);
    };

    // Everything below rejects misuse with a message naming the accepted forms.
    ($($rest:tt)*) => {
        ::std::compile_error!(
            "verbatim_builtin!: expected `verbatim_builtin!(Type, synopsis = \"...\", \
             description = \"...\")` or `verbatim_builtin!(Type, args = field, synopsis = \
             \"...\", description = \"...\")`, each optionally followed by `help = \"...\"`. \
             Options must appear in that order"
        );
    };
}

/// Implements [`HelpContent`](brush_core::builtins::HelpContent) from fixed
/// strings. Internal to [`verbatim_builtin!`]; not part of the API.
#[doc(hidden)]
#[macro_export]
macro_rules! __verbatim_builtin_help {
    ($t:ty, $synopsis:expr, $description:expr $(, $help:expr)?) => {
        impl $crate::__brush_core::builtins::HelpContent for $t {
            fn synopsis(_name: &str) -> ::std::string::String {
                ::std::string::String::from($synopsis)
            }

            fn description(_name: &str) -> ::std::string::String {
                ::std::string::String::from($description)
            }

            $(
            fn detailed_help(
                _name: &str,
                _options: &$crate::__brush_core::builtins::ContentOptions,
            ) -> ::std::result::Result<::std::string::String, $crate::__brush_core::Error> {
                ::std::result::Result::Ok(::std::string::String::from($help))
            }
            )?
        }
    };
}

#[cfg(test)]
mod tests {
    use brush_core::CommandArg;
    use brush_core::builtins::{ContentOptions, FromArgs, HelpContent};

    fn strings(words: &[&str]) -> Vec<CommandArg> {
        words
            .iter()
            .map(|w| CommandArg::String((*w).to_owned()))
            .collect()
    }

    struct Nullary {}

    crate::verbatim_builtin!(
        Nullary,
        synopsis = "nullary",
        description = "does nothing",
        help = "Does nothing at all.",
    );

    struct Passthrough {
        args: Vec<CommandArg>,
    }

    crate::verbatim_builtin!(
        Passthrough,
        args = args,
        synopsis = "passthrough [arg ...]",
        description = "keeps its arguments",
    );

    #[test]
    #[allow(clippy::panic)]
    fn nullary_ignores_arguments() {
        // Nothing here is inspected, so nothing can be rejected.
        Nullary::from_args("nullary", strings(&["--bogus", "a=1"]))
            .unwrap_or_else(|e| panic!("construction should succeed: {e}"));
        const { assert!(!<Nullary as FromArgs>::TAKES_DECLARATIONS) };
    }

    #[test]
    #[allow(clippy::panic)]
    fn passthrough_stores_arguments_verbatim() {
        let parsed = Passthrough::from_args("passthrough", strings(&["declare", "--bogus", "a=1"]))
            .unwrap_or_else(|e| panic!("construction should succeed: {e}"));
        assert_eq!(
            crate::args::into_words(parsed.args),
            ["declare", "--bogus", "a=1"]
        );
        const { assert!(<Passthrough as FromArgs>::TAKES_DECLARATIONS) };
    }

    #[test]
    #[expect(clippy::panic_in_result_fn)]
    fn help_is_fixed_text() -> Result<(), brush_core::Error> {
        assert_eq!(Nullary::synopsis("x"), "nullary");
        assert_eq!(Nullary::description("x"), "does nothing");
        assert_eq!(
            Nullary::detailed_help("x", &ContentOptions::default())?,
            "Does nothing at all."
        );
        // Without `help =`, the trait's default composes the short forms.
        assert_eq!(
            Passthrough::detailed_help("passthrough", &ContentOptions::default())?,
            "passthrough: passthrough [arg ...]\n    keeps its arguments\n"
        );
        Ok(())
    }
}
