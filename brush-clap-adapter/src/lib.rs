//! Binds builtins that use [clap](https://docs.rs/clap) to the engine-neutral
//! contract in [`brush_core::builtins`].
//!
//! A builtin opts in by deriving [`clap::Parser`] and invoking
//! [`impl_parse_args`]; the macro implements
//! [`brush_core::builtins::ParseArgs`], its parsing and its help. That macro,
//! and [`to_usage_error`] and [`try_parse_before_double_dash`] for a builtin
//! that runs clap itself, are this crate's API.
//!
//! `brush-core` does not depend on clap; a builtin that uses a different engine
//! implements [`ParseArgs`](brush_core::builtins::ParseArgs) some other way and
//! never touches this crate.

mod adapter;

pub use adapter::{to_usage_error, try_parse_before_double_dash};

/// What the exported macros expand to. Not part of the API, so it can change
/// in any release.
#[doc(hidden)]
pub mod __private {
    // Lets the macros name brush-core items from any calling crate.
    pub use brush_core;

    pub use crate::adapter::{
        content, parse, parse_declarations, parse_with_double_dash, parse_with_plus_options,
    };
}

/// Declares that a builtin uses clap: implements
/// [`ParseArgs`](brush_core::builtins::ParseArgs), its parsing and its help, for
/// a type that derives [`clap::Parser`].
///
/// # Forms
///
/// | Form | Parsing |
/// |---|---|
/// | `impl_parse_args!(Type)` | Every argument is flattened to a word and parsed by clap. |
/// | `impl_parse_args!(Type, plus_options)` | As above, but options may also start with `+`: each `+x` reaches clap as the long option `--+x`. |
/// | `impl_parse_args!(Type, keep_double_dash = field)` | The first `--` and every word after it are appended to `field` (a `Vec<String>`) verbatim, instead of letting clap consume the separator. |
/// | `impl_parse_args!(Type, declarations = field)` | A declaration utility: words that look like options are parsed by clap, and every other argument is kept in `field` (a `Vec<CommandArg>`) as it arrived. Add `, plus_options` for `+` options. |
/// | `impl_parse_args!(Type, parse = parser)` | `parser` (a function or closure from an [`Invocation`](brush_core::builtins::Invocation) to `Result<Type, UsageError>`) parses the arguments by hand; only the help comes from clap. Add `, declaration_utility` for a declaration utility. |
///
/// # Examples
///
/// ```
/// #[derive(clap::Parser)]
/// struct GreetCommand {
///     names: Vec<String>,
/// }
///
/// brush_clap_adapter::impl_parse_args!(GreetCommand);
///
/// #[derive(clap::Parser)]
/// struct EchoCommand {
///     #[arg(allow_hyphen_values = true)]
///     args: Vec<String>,
/// }
///
/// brush_clap_adapter::impl_parse_args!(EchoCommand, keep_double_dash = args);
/// ```
#[macro_export]
macro_rules! impl_parse_args {
    ($t:ty $(,)?) => {
        $crate::__impl_parse_args!($t, role = None, invocation => {
            $crate::__private::parse::<Self>(invocation)
        });
    };

    ($t:ty, plus_options $(,)?) => {
        $crate::__impl_parse_args!($t, role = None, invocation => {
            $crate::__private::parse_with_plus_options::<Self>(invocation)
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
            let (mut this, declarations) =
                $crate::__private::parse_declarations::<Self>(invocation, false)?;
            this.$field = declarations;
            ::std::result::Result::Ok(this)
        });
    };

    ($t:ty, declarations = $field:ident, plus_options $(,)?) => {
        $crate::__impl_parse_args!($t, role = Utility, invocation => {
            let (mut this, declarations) =
                $crate::__private::parse_declarations::<Self>(invocation, true)?;
            this.$field = declarations;
            ::std::result::Result::Ok(this)
        });
    };

    ($t:ty, parse = $parse:expr $(,)?) => {
        $crate::__impl_parse_args!($t, role = None, invocation => {
            ($parse)(invocation)
        });
    };

    ($t:ty, parse = $parse:expr, declaration_utility $(,)?) => {
        $crate::__impl_parse_args!($t, role = Utility, invocation => {
            ($parse)(invocation)
        });
    };
}

/// Implements [`ParseArgs`](brush_core::builtins::ParseArgs) for a
/// clap-derived type, with the given body of `parse`. Internal to
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
