//! Binds builtins that use [winnow-args](https://github.com/lu-zero/winnow-args)
//! to the engine-neutral contract in [`brush_core::builtins`].
//!
//! A builtin opts in by deriving [`winnow_args::Args`] and invoking
//! [`impl_parse_args`]; the macro implements
//! [`brush_core::builtins::ParseArgs`], its parsing and its help. That macro,
//! and [`to_usage_error`] for a builtin that parses by hand, are this crate's
//! API. Its forms
//! are a subset of `brush_clap_adapter::impl_parse_args!`'s: `+x` options are
//! winnow-args' own (`#[arg(plus_options)]`), so they need no form of their
//! own.
//!
//! The words are parsed where they lie, borrowed rather than copied. Errors
//! are winnow-args' own (`name: error: …`), followed by the usage line.

mod adapter;

pub use adapter::{HELP_PALETTE, to_usage_error};

/// What the exported macros expand to. Not part of the API, so it can change
/// in any release.
#[doc(hidden)]
pub mod __private {
    // Lets the macros name brush-core items from any calling crate.
    pub use brush_core;

    pub use crate::adapter::{content, parse, parse_declarations, parse_with_double_dash};
}

/// Declares that a builtin uses winnow-args: implements
/// [`ParseArgs`](brush_core::builtins::ParseArgs), its parsing and its help, for
/// a type that derives [`winnow_args::Args`].
///
/// # Forms
///
/// | Form | Parsing |
/// |---|---|
/// | `impl_parse_args!(Type)` | Every argument is flattened to a word and parsed. |
/// | `impl_parse_args!(Type, keep_double_dash = field)` | The first `--` and every word after it are appended to `field` (a `Vec<String>`) verbatim. |
/// | `impl_parse_args!(Type, declarations = field)` | A declaration utility: the options up to the first operand or `--` are parsed, and the operands are kept in `field` (a `Vec<CommandArg>`, marked `#[arg(skip)]`) as they arrived. |
/// | `impl_parse_args!(Type, parse = parser)` | `parser` (a function or closure from an [`Invocation`](brush_core::builtins::Invocation) to `Result<Type, UsageError>`) parses the arguments by hand; only the help comes from winnow-args. |
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
/// brush_winnow_args_adapter::impl_parse_args!(GreetCommand);
/// ```
#[macro_export]
macro_rules! impl_parse_args {
    ($t:ty $(,)?) => {
        $crate::__impl_parse_args!($t, role = None, invocation => {
            $crate::__private::parse::<Self>(invocation)
        });
    };

    ($t:ty, keep_double_dash = $field:ident $(,)?) => {
        $crate::__impl_parse_args!($t, role = None, invocation => {
            let (mut this, trailing) = $crate::__private::parse_with_double_dash::<Self>(invocation)?;
            this.$field.extend(trailing);
            ::std::result::Result::Ok(this)
        });
    };

    ($t:ty, declarations = $field:ident $(,)?) => {
        $crate::__impl_parse_args!($t, role = Utility, invocation => {
            let (mut this, declarations) = $crate::__private::parse_declarations::<Self>(invocation)?;
            this.$field = declarations;
            ::std::result::Result::Ok(this)
        });
    };

    ($t:ty, parse = $parse:expr $(,)?) => {
        $crate::__impl_parse_args!($t, role = None, invocation => {
            ($parse)(invocation)
        });
    };
}

/// Implements [`ParseArgs`](brush_core::builtins::ParseArgs) for a
/// winnow-args type, with the given body of `parse`. Internal to
/// [`impl_parse_args!`]; not part of the API.
#[doc(hidden)]
#[macro_export]
macro_rules! __impl_parse_args {
    ($t:ty, role = $role:ident, $invocation:ident => $parse:block) => {
        impl $crate::__private::brush_core::builtins::ParseArgs for $t {
            const DECLARATION_ROLE: $crate::__private::brush_core::builtins::DeclarationRole =
                $crate::__private::brush_core::builtins::DeclarationRole::$role;

            fn content(
                name: &str,
                content_type: $crate::__private::brush_core::builtins::ContentType,
                options: &$crate::__private::brush_core::builtins::ContentOptions,
            ) -> ::std::result::Result<::std::string::String, $crate::__private::brush_core::Error>
            {
                $crate::__private::content::<Self>(name, content_type, options)
            }

            fn parse(
                $invocation: $crate::__private::brush_core::builtins::Invocation<'_>,
            ) -> ::std::result::Result<Self, $crate::__private::brush_core::builtins::UsageError>
            {
                $parse
            }
        }
    };
}
