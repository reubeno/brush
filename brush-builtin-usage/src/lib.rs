//! Binds builtins that use [usage](https://usage.jdx.dev) to the engine-neutral
//! contracts in [`brush_core`].
//!
//! `brush-core` does not depend on a parsing engine. A builtin opts in by
//! deriving [`usage::Cli`] and invoking [`usage_builtin`]; the macro implements
//! [`brush_core::builtins::FromArgs`] and [`brush_core::builtins::HelpContent`].
//! The three forms match [`brush_builtin_utils::clap_builtin`]: plain parsing,
//! a verbatim `--` tail, and declaration operands left unparsed.
//!
//! Shell conventions that neither engine knows about — leading `+` options, the
//! operand boundary of `declare` and `export` — live in this adapter, on top of
//! the same helpers `clap_builtin!` uses.

pub mod adapter;

#[doc(hidden)]
pub use brush_core as __brush_core;
#[doc(hidden)]
pub use usage as __usage;

pub use adapter::UsageParsed;

/// Declares that a builtin uses usage-rs, wiring up its argument parsing and
/// its help content. Expects the type to derive [`usage::Cli`].
///
/// # Forms
///
/// Exactly one of these forms is used per type. The options are mutually
/// exclusive, and any other option is a compile error naming the accepted
/// forms. They are the same three modes as `clap_builtin!`.
///
/// | Form | Parsing |
/// |---|---|
/// | `usage_builtin!(Type)` | Every argument is flattened to a string and parsed by usage. |
/// | `usage_builtin!(Type, trailing_args = field)` | As above, but the first `--` and every word after it are taken verbatim and appended to `field` (a `Vec<String>`). For `echo`, `test`, `set`, `getopts`. |
/// | `usage_builtin!(Type, declarations = field)` | Only the leading options are parsed; the operands after them are stored in `field` (a `Vec<CommandArg>`, marked `#[usage(skip)]`) with assignments intact, and [`FromArgs::TAKES_DECLARATIONS`](brush_core::builtins::FromArgs::TAKES_DECLARATIONS) is set. For `declare`, `export`, and similar. |
///
/// # Examples
///
/// ```
/// use usage::Cli;
///
/// /// Greet someone.
/// #[derive(Cli)]
/// #[usage(bin = "greet", disable_help_flag, disable_version_flag)]
/// struct GreetCommand {
///     /// Names to greet.
///     names: Vec<String>,
/// }
///
/// brush_builtin_usage::usage_builtin!(GreetCommand);
/// ```
#[macro_export]
macro_rules! usage_builtin {
    ($t:ty $(,)?) => {
        impl $crate::UsageParsed for $t {
            fn parse_argv<'v>(
                argv: &[&'v ::std::ffi::OsStr],
            ) -> ::std::result::Result<Self, $crate::__usage::Error<'static, 'v>> {
                <$t>::parse_from(argv)
            }

            fn spec() -> &'static $crate::__usage::spec::Spec<'static> {
                <$t>::spec()
            }

            fn command() -> &'static $crate::__usage::Command<'static> {
                <$t>::command()
            }
        }

        impl $crate::__brush_core::builtins::FromArgs for $t {
            fn from_args(
                name: &str,
                args: ::std::vec::Vec<$crate::__brush_core::CommandArg>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::ArgsError> {
                $crate::adapter::parse::<Self>(name, args)
            }
        }

        $crate::__usage_builtin_help!($t);
    };

    ($t:ty, trailing_args = $field:ident $(,)?) => {
        impl $crate::UsageParsed for $t {
            fn parse_argv<'v>(
                argv: &[&'v ::std::ffi::OsStr],
            ) -> ::std::result::Result<Self, $crate::__usage::Error<'static, 'v>> {
                <$t>::parse_from(argv)
            }

            fn spec() -> &'static $crate::__usage::spec::Spec<'static> {
                <$t>::spec()
            }

            fn command() -> &'static $crate::__usage::Command<'static> {
                <$t>::command()
            }
        }

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

        $crate::__usage_builtin_help!($t);
    };

    ($t:ty, declarations = $field:ident $(,)?) => {
        impl $crate::UsageParsed for $t {
            fn parse_argv<'v>(
                argv: &[&'v ::std::ffi::OsStr],
            ) -> ::std::result::Result<Self, $crate::__usage::Error<'static, 'v>> {
                <$t>::parse_from(argv)
            }

            fn spec() -> &'static $crate::__usage::spec::Spec<'static> {
                <$t>::spec()
            }

            fn command() -> &'static $crate::__usage::Command<'static> {
                <$t>::command()
            }
        }

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

        $crate::__usage_builtin_help!($t);
    };

    ($t:ty, trailing_args = $a:ident, declarations = $($rest:tt)*) => {
        ::std::compile_error!(
            "usage_builtin!: `trailing_args` and `declarations` are mutually exclusive; \
             a builtin is parsed in one mode. Accepted forms: `usage_builtin!(Type)`, \
             `usage_builtin!(Type, trailing_args = field)`, \
             `usage_builtin!(Type, declarations = field)`"
        );
    };
    ($t:ty, declarations = $a:ident, trailing_args = $($rest:tt)*) => {
        ::std::compile_error!(
            "usage_builtin!: `trailing_args` and `declarations` are mutually exclusive; \
             a builtin is parsed in one mode. Accepted forms: `usage_builtin!(Type)`, \
             `usage_builtin!(Type, trailing_args = field)`, \
             `usage_builtin!(Type, declarations = field)`"
        );
    };
    ($t:ty, trailing_args = $($rest:tt)*) => {
        ::std::compile_error!(
            "usage_builtin!: `trailing_args` takes a field name, as in \
             `usage_builtin!(Type, trailing_args = field)`, where `field` is a `Vec<String>`"
        );
    };
    ($t:ty, declarations = $($rest:tt)*) => {
        ::std::compile_error!(
            "usage_builtin!: `declarations` takes a field name, as in \
             `usage_builtin!(Type, declarations = field)`, where `field` is a \
             `Vec<brush_core::CommandArg>` marked `#[usage(skip)]`"
        );
    };
    ($t:ty, $option:ident = $($rest:tt)*) => {
        ::std::compile_error!(::std::concat!(
            "usage_builtin!: unknown option `",
            ::std::stringify!($option),
            "`. Accepted forms: `usage_builtin!(Type)`, \
             `usage_builtin!(Type, trailing_args = field)`, \
             `usage_builtin!(Type, declarations = field)`"
        ));
    };
    ($($rest:tt)*) => {
        ::std::compile_error!(
            "usage_builtin!: expected a type that derives `usage::Cli`, optionally followed \
             by one option. Accepted forms: `usage_builtin!(Type)`, \
             `usage_builtin!(Type, trailing_args = field)`, \
             `usage_builtin!(Type, declarations = field)`"
        );
    };
}

/// Implements [`HelpContent`](brush_core::builtins::HelpContent) for a
/// usage-derived type. Internal to [`usage_builtin`]; not part of the API.
#[doc(hidden)]
#[macro_export]
macro_rules! __usage_builtin_help {
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
