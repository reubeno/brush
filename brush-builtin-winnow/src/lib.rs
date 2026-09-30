//! Binds builtins that use [winnow-args](https://github.com/lu-zero/winnow-args)
//! to the engine-neutral contracts in [`brush_core`].
//!
//! A builtin opts in by deriving [`winnow_args::Args`] and invoking
//! [`winnow_builtin`]; the macro implements
//! [`brush_core::builtins::FromArgs`] and [`brush_core::builtins::HelpContent`].
//! The three forms match `clap_builtin!`'s: plain parsing, a
//! verbatim `--` tail, and declaration operands left unparsed.
//!
//! The words are parsed where they lie, borrowed rather than copied; `+x`
//! options are winnow-args' own (`#[arg(plus_options)]`), so nothing is
//! rewritten first. Errors read as bash's builtins print them:
//! `name: -x: invalid option`, then `name: usage: ...`.

pub mod adapter;

#[doc(hidden)]
pub use brush_core as __brush_core;
#[doc(hidden)]
pub use winnow_args as __winnow_args;

/// Declares that a builtin uses winnow-args, wiring up its argument parsing
/// and its help content. Expects the type to derive [`winnow_args::Args`].
///
/// # Forms
///
/// | Form | Parsing |
/// |---|---|
/// | `winnow_builtin!(Type)` | Every argument is flattened to a word and parsed. |
/// | `winnow_builtin!(Type, trailing_args = field)` | As above, but the first `--` and every word after it are taken verbatim and appended to `field` (a `Vec<String>`). |
/// | `winnow_builtin!(Type, declarations = field)` | Only the leading options are parsed; the operands after them are stored in `field` (a `Vec<CommandArg>`, marked `#[arg(skip)]`) with assignments intact, and [`FromArgs::TAKES_DECLARATIONS`](brush_core::builtins::FromArgs::TAKES_DECLARATIONS) is set. |
///
/// # Examples
///
/// ```
/// /// Greet someone.
/// #[derive(winnow_args::Args)]
/// #[arg(disable_help_flag, disable_version_flag)]
/// struct GreetCommand {
///     /// Names to greet.
///     #[arg(positional)]
///     names: Vec<String>,
/// }
///
/// brush_builtin_winnow::winnow_builtin!(GreetCommand);
/// ```
#[macro_export]
macro_rules! winnow_builtin {
    ($t:ty $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            fn from_args(
                name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                $crate::adapter::parse::<Self>(name, args)
            }
        }

        $crate::__winnow_builtin_help!($t);
    };

    ($t:ty, trailing_args = $field:ident $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            fn from_args(
                name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                let (mut this, trailing) =
                    $crate::adapter::parse_with_trailing::<Self>(name, args)?;
                this.$field.extend(trailing);
                Ok(this)
            }
        }

        $crate::__winnow_builtin_help!($t);
    };

    ($t:ty, declarations = $field:ident $(,)?) => {
        impl $crate::__brush_core::builtins::FromArgs for $t {
            const TAKES_DECLARATIONS: bool = true;

            fn from_args(
                name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                let (mut this, declarations) =
                    $crate::adapter::parse_with_declarations::<Self>(name, args)?;
                this.$field = declarations;
                Ok(this)
            }
        }

        $crate::__winnow_builtin_help!($t);
    };

    ($($rest:tt)*) => {
        ::std::compile_error!(
            "winnow_builtin!: expected a type that derives `winnow_args::Args`, optionally \
             followed by one option. Accepted forms: `winnow_builtin!(Type)`, \
             `winnow_builtin!(Type, trailing_args = field)`, \
             `winnow_builtin!(Type, declarations = field)`"
        );
    };
}

/// Implements [`HelpContent`](brush_core::builtins::HelpContent) for a
/// winnow-args type. Internal to [`winnow_builtin`]; not part of the API.
#[doc(hidden)]
#[macro_export]
macro_rules! __winnow_builtin_help {
    ($t:ty) => {
        impl $crate::__brush_core::builtins::HelpContent for $t {
            fn synopsis(name: &str) -> ::std::string::String {
                $crate::adapter::synopsis::<Self>(name)
            }

            fn description(name: &str) -> ::std::string::String {
                $crate::adapter::description::<Self>(name)
            }

            fn detailed_help(
                name: &str,
                options: &$crate::__brush_core::builtins::ContentOptions,
            ) -> ::std::result::Result<::std::string::String, $crate::__brush_core::Error> {
                $crate::adapter::detailed_help::<Self>(name, options)
            }
        }
    };
}
