//! Standard builtins.

// `brush_core::builtins::Command::execute` is async by contract: the trait declares a desugared
// `-> impl Future<...> + Send` so that `brush_core::builtins::exec_builtin` can box and dispatch
// every builtin uniformly. Most builtins in this crate do purely synchronous work, so their
// `execute` bodies contain no `.await` -- that is the trait contract being honored, not a defect.
#![allow(
    clippy::unused_async_trait_impl,
    reason = "builtins implement a trait whose `execute` is async by contract"
)]

// N.B. Builtins carry argument instrumentation for every supported parsing
// engine via `cfg_attr`; exactly one engine feature may be active per build.
#[cfg(all(
    feature = "parser-clap",
    any(feature = "parser-bpaf", feature = "parser-usage")
))]
compile_error!("select at most one builtin argument-parsing engine feature");
#[cfg(not(any(
    feature = "parser-clap",
    feature = "parser-bpaf",
    feature = "parser-usage"
)))]
compile_error!("select one builtin argument-parsing engine feature");

/// Tri-state bpaf parser for a `-x` / `+x` option pair: `None` when absent,
/// `Some(true)` for `-x`, `Some(false)` for `+x`.
pub(crate) fn minus_or_plus_flag(
    flag_char: char,
    plus_form: &'static str,
    desc: &'static str,
) -> impl bpaf::Parser<Option<bool>> {
    use bpaf::Parser;

    let enable = bpaf::short(flag_char)
        .help(desc)
        .switch()
        .map(|enabled| enabled.then_some(true));
    let disable = bpaf::literal(plus_form)
        .help("Disables the flag.")
        .hide()
        .map(|(): ()| Some(false));

    bpaf::construct!([enable, disable]).fallback(None)
}

/// Declares a bpaf-engine tri-state flag struct mirroring
/// `minus_or_plus_flag_arg`'s clap-side shape.
macro_rules! minus_or_plus_flag_bpaf {
    ($struct_name:ident) => {
        #[derive(Default)]
        pub(crate) struct $struct_name {
            pub(super) enable: bool,
            pub(super) disable: bool,
        }

        impl $struct_name {
            #[allow(dead_code, reason = "engine-side constructor")]
            pub(crate) fn from_bool(value: Option<bool>) -> Self {
                let mut this = Self {
                    enable: false,
                    disable: false,
                };

                match value {
                    Some(true) => this.enable = true,
                    Some(false) => this.disable = true,
                    None => {}
                }

                this
            }

            pub const fn to_bool(&self) -> Option<bool> {
                match (self.enable, self.disable) {
                    (true, false) => Some(true),
                    (false, true) => Some(false),
                    _ => None,
                }
            }
        }
    };
}

pub(crate) use minus_or_plus_flag_bpaf;

/// Selects a builtin's argument implementation according to the active
/// argument-parsing engine feature (`parser-clap` / `parser-bpaf` /
/// `parser-usage`; exactly one must be enabled).
///
/// Expands to declarations of the per-engine sibling modules plus a re-export
/// of `$t` from whichever engine module is compiled, so the rest of the
/// builtin's file can refer to the type engine-independently.
macro_rules! arg_impl {
    ($t:ident) => {
        #[cfg(feature = "parser-bpaf")]
        pub mod bpaf;
        #[cfg(feature = "parser-clap")]
        pub mod clap;
        #[cfg(feature = "parser-usage")]
        pub mod usage;

        mod imp {
            #[cfg(feature = "parser-bpaf")]
            pub(crate) use super::bpaf::*;
            #[cfg(feature = "parser-clap")]
            pub(crate) use super::clap::*;
            #[cfg(feature = "parser-usage")]
            pub(crate) use super::usage::*;
        }

        pub(crate) use imp::$t;
    };
}

#[cfg(feature = "parser-bpaf")]
pub(crate) mod args;

#[cfg(feature = "builtin.alias")]
mod alias;
#[cfg(feature = "builtin.bg")]
mod bg;
#[cfg(feature = "builtin.bind")]
mod bind;
#[cfg(feature = "builtin.break")]
mod break_;
#[cfg(feature = "builtin.builtin")]
mod builtin_;
#[cfg(feature = "builtin.caller")]
mod caller;
#[cfg(feature = "builtin.cd")]
mod cd;
#[cfg(feature = "builtin.colon")]
mod colon;
#[cfg(feature = "builtin.command")]
mod command;
#[cfg(any(
    feature = "builtin.complete",
    feature = "builtin.compgen",
    feature = "builtin.compopt"
))]
mod complete;
#[cfg(feature = "builtin.continue")]
mod continue_;
#[cfg(feature = "builtin.declare")]
mod declare;
#[cfg(feature = "builtin.dirs")]
mod dirs;
#[cfg(feature = "builtin.dot")]
mod dot;
#[cfg(feature = "builtin.echo")]
mod echo;
#[cfg(feature = "builtin.enable")]
mod enable;
#[cfg(feature = "builtin.eval")]
mod eval;
#[cfg(all(feature = "builtin.exec", unix))]
mod exec;
#[cfg(feature = "builtin.exit")]
mod exit;
#[cfg(feature = "builtin.export")]
mod export;
#[cfg(feature = "builtin.false")]
mod false_;
#[cfg(feature = "builtin.fc")]
mod fc;
#[cfg(feature = "builtin.fg")]
mod fg;
#[cfg(feature = "builtin.getopts")]
mod getopts;
#[cfg(feature = "builtin.hash")]
mod hash;
#[cfg(feature = "builtin.help")]
mod help;
#[cfg(feature = "builtin.history")]
mod history;
#[cfg(feature = "builtin.jobs")]
mod jobs;
#[cfg(all(feature = "builtin.kill", unix))]
mod kill;
#[cfg(feature = "builtin.let")]
mod let_;
#[cfg(feature = "builtin.mapfile")]
mod mapfile;
#[cfg(feature = "builtin.popd")]
mod popd;
#[cfg(all(feature = "builtin.printf", any(unix, windows)))]
mod printf;
#[cfg(feature = "builtin.pushd")]
mod pushd;
#[cfg(feature = "builtin.pwd")]
mod pwd;
#[cfg(feature = "builtin.read")]
mod read;
#[cfg(feature = "builtin.return")]
mod return_;
#[cfg(feature = "builtin.set")]
mod set;
#[cfg(feature = "builtin.shift")]
mod shift;
#[cfg(feature = "builtin.shopt")]
mod shopt;
#[cfg(all(feature = "builtin.suspend", unix))]
mod suspend;
#[cfg(feature = "builtin.test")]
mod test;
#[cfg(feature = "builtin.times")]
mod times;
#[cfg(feature = "builtin.trap")]
mod trap;
#[cfg(feature = "builtin.true")]
mod true_;
#[cfg(feature = "builtin.type")]
mod type_;
#[cfg(all(feature = "builtin.ulimit", unix))]
mod ulimit;
#[cfg(all(feature = "builtin.umask", unix))]
mod umask;
#[cfg(feature = "builtin.unalias")]
mod unalias;
#[cfg(feature = "builtin.unset")]
mod unset;
#[cfg(feature = "builtin.wait")]
mod wait;

mod builder;
mod factory;
mod unimp;

pub use builder::ShellBuilderExt;
pub use factory::{BuiltinSet, default_builtins};

/// Macro to define a struct that represents a shell built-in flag argument that can be
/// enabled or disabled by specifying an option with a leading '+' or '-' character.
///
/// # Arguments
///
/// - `$struct_name` - The identifier to be used for the struct to define.
/// - `$flag_char` - The character to use as the flag.
/// - `$desc` - The string description of the flag.
#[macro_export]
macro_rules! minus_or_plus_flag_arg {
    ($struct_name:ident, $flag_char:literal, $desc:literal) => {
        #[derive(clap::Parser)]
        pub(crate) struct $struct_name {
            #[arg(short = $flag_char, name = concat!(stringify!($struct_name), "_enable"), action = clap::ArgAction::SetTrue, help = $desc)]
            _enable: bool,
            #[arg(long = concat!("+", $flag_char), name = concat!(stringify!($struct_name), "_disable"), action = clap::ArgAction::SetTrue, hide = true)]
            _disable: bool,
        }

        impl From<$struct_name> for Option<bool> {
            fn from(value: $struct_name) -> Self {
                value.to_bool()
            }
        }

        impl $struct_name {
            #[allow(dead_code, reason = "may not be used in all macro instantiations")]
            pub const fn is_some(&self) -> bool {
                self._enable || self._disable
            }

            /// Constructs the tri-state flag from an engine-parsed value.
            #[allow(dead_code, reason = "used by non-clap engine modules")]
            pub(crate) fn from_bool(value: Option<bool>) -> Self {
                let mut this = Self {
                    _enable: false,
                    _disable: false,
                };

                match value {
                    Some(true) => this._enable = true,
                    Some(false) => this._disable = true,
                    None => {}
                }

                this
            }

            pub const fn to_bool(&self) -> Option<bool> {
                match (self._enable, self._disable) {
                    (true, false) => Some(true),
                    (false, true) => Some(false),
                    _ => None,
                }
            }
        }
    };
}
