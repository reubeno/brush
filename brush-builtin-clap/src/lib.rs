//! Binds builtins that use [clap](https://docs.rs/clap) to the engine-neutral
//! contract in [`brush_core::builtins`].
//!
//! A builtin opts in by deriving [`clap::Parser`] and invoking
//! [`impl_parse_args`]; the macro implements
//! [`brush_core::builtins::ParseArgs`], its parsing and its help. That macro,
//! and [`to_usage_error`] for a builtin that parses by hand, are this crate's
//! API.
//!
//! `brush-core` does not depend on clap; a builtin that uses a different engine
//! implements [`ParseArgs`](brush_core::builtins::ParseArgs) some other way and
//! never touches this crate. Failures become structured
//! [`UsageError`](brush_core::builtins::UsageError)s, which the shell reports.
//!
//! A builtin's parse should see only its words, so don't give an argument
//! clap's `env = "..."`: it reads the process's environment, which isn't the
//! shell's variables.

#[doc(hidden)]
pub mod adapter;
mod text;

pub use adapter::to_usage_error;

// Lets the exported macros name brush-core items from any calling crate.
#[doc(hidden)]
pub use brush_core as __brush_core;

/// Declares that a builtin uses clap: implements
/// [`ParseArgs`](brush_core::builtins::ParseArgs), its parsing and its help, for
/// a type that derives [`clap::Parser`].
///
/// # Forms
///
/// Exactly one of these forms is used per type; the options are mutually
/// exclusive, and any other option is a compile error naming the accepted
/// forms.
///
/// | Form | Parsing |
/// |---|---|
/// | `impl_parse_args!(Type)` | Every argument is flattened to a word and parsed by clap. |
/// | `impl_parse_args!(Type, keep_double_dash = field)` | As above, but the first `--` and every word after it are appended to `field` (a `Vec<String>`) verbatim, instead of letting clap consume the separator. For `echo`, `getopts`, `set`, and `test`. |
/// | `impl_parse_args!(Type, help_from = HelpType, parse = parser)` | `parser` (a function or closure from an [`Invocation`](brush_core::builtins::Invocation) to `Result<Type, UsageError>`) parses the words by hand; the help is `HelpType`'s, a type that derives [`clap::Parser`] only for it. |
///
/// Whether the builtin is a declaration utility is said separately, by
/// [`StatelessBuiltin::DECLARATIONS`](brush_core::builtins::StatelessBuiltin::DECLARATIONS).
///
/// A builtin that parses nothing at all doesn't need clap: it implements
/// [`ParseArgs`](brush_core::builtins::ParseArgs) by hand, in a few lines.
///
/// # Examples
///
/// ```
/// #[derive(clap::Parser)]
/// struct GreetCommand {
///     names: Vec<String>,
/// }
///
/// brush_builtin_clap::impl_parse_args!(GreetCommand);
///
/// #[derive(clap::Parser)]
/// struct EchoCommand {
///     #[arg(allow_hyphen_values = true)]
///     args: Vec<String>,
/// }
///
/// brush_builtin_clap::impl_parse_args!(EchoCommand, keep_double_dash = args);
/// ```
///
/// Misuse is rejected with a message naming the accepted forms:
///
/// ```compile_fail
/// # #[derive(clap::Parser)]
/// # struct T { a: Vec<String> }
/// brush_builtin_clap::impl_parse_args!(T, trailing = a);
/// ```
#[macro_export]
macro_rules! impl_parse_args {
    ($t:ty $(,)?) => {
        $crate::__impl_parse_args!($t, help = Self, invocation => {
            $crate::adapter::parse::<Self>(invocation)
        });
    };

    ($t:ty, keep_double_dash = $field:ident $(,)?) => {
        $crate::__impl_parse_args!($t, help = Self, invocation => {
            let (mut this, trailing) = $crate::adapter::parse_with_double_dash::<Self>(invocation)?;
            this.$field.extend(trailing);
            ::std::result::Result::Ok(this)
        });
    };

    ($t:ty, help_from = $help:ty, parse = $parse:expr $(,)?) => {
        $crate::__impl_parse_args!($t, help = $help, invocation => {
            ($parse)(invocation)
        });
    };

    // Everything below rejects misuse with a message naming the accepted forms.
    ($t:ty, keep_double_dash = $($rest:tt)*) => {
        ::std::compile_error!(
            "impl_parse_args!: `keep_double_dash` takes a field name, as in \
             `impl_parse_args!(Type, keep_double_dash = field)`, where `field` is a `Vec<String>`"
        );
    };
    ($t:ty, $option:ident = $($rest:tt)*) => {
        ::std::compile_error!(::std::concat!(
            "impl_parse_args!: unknown option `",
            ::std::stringify!($option),
            "`. Accepted forms: `impl_parse_args!(Type)`, \
             `impl_parse_args!(Type, keep_double_dash = field)`, \
             `impl_parse_args!(Type, help_from = HelpType, parse = parser)`"
        ));
    };
    ($($rest:tt)*) => {
        ::std::compile_error!(
            "impl_parse_args!: expected a type that derives `clap::Parser`, optionally followed \
             by options. Accepted forms: `impl_parse_args!(Type)`, \
             `impl_parse_args!(Type, keep_double_dash = field)`, \
             `impl_parse_args!(Type, help_from = HelpType, parse = parser)`"
        );
    };
}

/// Implements [`ParseArgs`](brush_core::builtins::ParseArgs) for a type, with
/// the help of a clap-derived type and the given body of `parse`. Internal to
/// [`impl_parse_args!`]; not part of the API.
#[doc(hidden)]
#[macro_export]
macro_rules! __impl_parse_args {
    ($t:ty, help = $help:ty, $invocation:ident => $parse:block) => {
        impl $crate::__brush_core::builtins::ParseArgs for $t {
            fn help(name: &str) -> $crate::__brush_core::builtins::Help {
                $crate::adapter::help::<$help>(name)
            }

            fn terminal_help(
                name: &str,
                style: &$crate::__brush_core::builtins::TerminalStyle,
            ) -> ::std::option::Option<$crate::__brush_core::builtins::Help> {
                ::std::option::Option::Some($crate::adapter::terminal_help::<$help>(name, style))
            }

            fn parse(
                $invocation: $crate::__brush_core::builtins::Invocation<'_>,
            ) -> ::std::result::Result<Self, $crate::__brush_core::builtins::UsageError> {
                $parse
            }
        }
    };
}
