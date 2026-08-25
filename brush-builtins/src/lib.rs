//! Standard builtins.

// `brush_core::builtins::Command::execute` is async by contract: the trait declares a desugared
// `-> impl Future<...> + Send` so that `brush_core::builtins::exec_builtin` can box and dispatch
// every builtin uniformly. Most builtins in this crate do purely synchronous work, so their
// `execute` bodies contain no `.await` -- that is the trait contract being honored, not a defect.
#![allow(
    clippy::unused_async_trait_impl,
    reason = "builtins implement a trait whose `execute` is async by contract"
)]

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

/// Declares the enable/disable pair for a shell built-in flag argument that
/// can be toggled with a leading '-' or '+' option (e.g., `-x` / `+x`).
///
/// Pairs with [`read_plus_minus`], which yields `None` when neither form is
/// present, `Some(true)` for `-x`, and `Some(false)` for `+x`.
pub(crate) fn declare_plus_minus(
    spec: brush_core::argmodel::CommandSpecBuilder,
    flag_char: char,
    base_id: &'static str,
    desc: &'static str,
) -> brush_core::argmodel::CommandSpecBuilder {
    use brush_core::argmodel::{ArgKind, Matches};

    let _ = Matches::new;
    let enable_id: &'static str = Box::leak(format!("{base_id}_enable").into_boxed_str());
    let disable_id: &'static str = Box::leak(format!("{base_id}_disable").into_boxed_str());
    let shorts: &'static [char] = Box::leak(vec![flag_char].into_boxed_slice());
    let plus_form: &'static str = Box::leak(format!("+{flag_char}").into_boxed_str());
    let longs: &'static [&'static str] = Box::leak(vec![plus_form].into_boxed_slice());

    spec.arg(enable_id, shorts, &[], ArgKind::Flag, None, desc)
        .hidden_arg(disable_id, &[], longs, ArgKind::Flag, None, "")
}

/// Reads back an optional enable/disable toggle declared by
/// [`declare_plus_minus`].
pub(crate) fn read_plus_minus(
    matches: &brush_core::argmodel::Matches,
    base_id: &str,
) -> Option<bool> {
    let enable = matches.flag(&format!("{base_id}_enable"));
    let disable = matches.flag(&format!("{base_id}_disable"));
    match (enable, disable) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}
