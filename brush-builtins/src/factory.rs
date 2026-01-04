use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

use brush_core::ShellRuntime;
#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use brush_core::builtins::{self, builtin, decl_builtin, raw_arg_builtin, simple_builtin};

/// Identifies well-known sets of builtins.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BuiltinSet {
    /// Identifies builtins appropriate for POSIX `sh` compatibility.
    ShMode,
    /// Identifies builtins appropriate for a more full-featured `bash`-compatible shell.
    BashMode,
}

/// Returns the default set of built-in commands.
///
/// # Arguments
///
/// * `set` - The set of built-ins to return.
#[allow(clippy::too_many_lines)]
pub fn default_builtins<S: ShellRuntime>(
    set: BuiltinSet,
) -> HashMap<String, builtins::Registration<S>> {
    let mut m = HashMap::<String, builtins::Registration<S>>::new();

    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    #[cfg(feature = "builtin.break")]
    m.insert(
        "break".into(),
        builtin::<break_::BreakCommand, S>().special(),
    );
    #[cfg(feature = "builtin.colon")]
    m.insert(
        ":".into(),
        simple_builtin::<colon::ColonCommand, S>().special(),
    );
    #[cfg(feature = "builtin.continue")]
    m.insert(
        "continue".into(),
        builtin::<continue_::ContinueCommand, S>().special(),
    );
    #[cfg(feature = "builtin.dot")]
    m.insert(".".into(), builtin::<dot::DotCommand, S>().special());
    #[cfg(feature = "builtin.eval")]
    m.insert("eval".into(), builtin::<eval::EvalCommand, S>().special());
    #[cfg(all(feature = "builtin.exec", unix))]
    m.insert("exec".into(), builtin::<exec::ExecCommand, S>().special());
    #[cfg(feature = "builtin.exit")]
    m.insert("exit".into(), builtin::<exit::ExitCommand, S>().special());
    #[cfg(feature = "builtin.export")]
    m.insert(
        "export".into(),
        decl_builtin::<export::ExportCommand, S>().special(),
    );
    #[cfg(feature = "builtin.return")]
    m.insert(
        "return".into(),
        builtin::<return_::ReturnCommand, S>().special(),
    );
    #[cfg(feature = "builtin.set")]
    m.insert("set".into(), builtin::<set::SetCommand, S>().special());
    #[cfg(feature = "builtin.shift")]
    m.insert(
        "shift".into(),
        builtin::<shift::ShiftCommand, S>().special(),
    );
    #[cfg(feature = "builtin.trap")]
    m.insert("trap".into(), builtin::<trap::TrapCommand, S>().special());
    #[cfg(feature = "builtin.unset")]
    m.insert(
        "unset".into(),
        builtin::<unset::UnsetCommand, S>().special(),
    );

    #[cfg(feature = "builtin.declare")]
    m.insert(
        "readonly".into(),
        decl_builtin::<declare::DeclareCommand, S>().special(),
    );
    #[cfg(feature = "builtin.times")]
    m.insert(
        "times".into(),
        builtin::<times::TimesCommand, S>().special(),
    );

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    m.insert("alias".into(), builtin::<alias::AliasCommand, S>()); // TODO(alias): should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    m.insert("bg".into(), builtin::<bg::BgCommand, S>());
    #[cfg(feature = "builtin.cd")]
    m.insert("cd".into(), builtin::<cd::CdCommand, S>());
    #[cfg(feature = "builtin.command")]
    m.insert("command".into(), builtin::<command::CommandCommand, S>());
    #[cfg(feature = "builtin.false")]
    m.insert("false".into(), simple_builtin::<false_::FalseCommand, S>());
    #[cfg(feature = "builtin.fg")]
    m.insert("fg".into(), builtin::<fg::FgCommand, S>());
    #[cfg(feature = "builtin.getopts")]
    m.insert("getopts".into(), builtin::<getopts::GetOptsCommand, S>());
    #[cfg(feature = "builtin.hash")]
    m.insert("hash".into(), builtin::<hash::HashCommand, S>());
    #[cfg(feature = "builtin.help")]
    m.insert("help".into(), builtin::<help::HelpCommand, S>());
    #[cfg(feature = "builtin.jobs")]
    m.insert("jobs".into(), builtin::<jobs::JobsCommand, S>());
    #[cfg(all(feature = "builtin.kill", unix))]
    m.insert("kill".into(), builtin::<kill::KillCommand, S>());
    #[cfg(feature = "builtin.declare")]
    m.insert("local".into(), decl_builtin::<declare::DeclareCommand, S>());
    #[cfg(feature = "builtin.pwd")]
    m.insert("pwd".into(), builtin::<pwd::PwdCommand, S>());
    #[cfg(feature = "builtin.read")]
    m.insert("read".into(), builtin::<read::ReadCommand, S>());
    #[cfg(feature = "builtin.true")]
    m.insert("true".into(), simple_builtin::<true_::TrueCommand, S>());
    #[cfg(feature = "builtin.type")]
    m.insert("type".into(), builtin::<type_::TypeCommand, S>());
    #[cfg(all(feature = "builtin.ulimit", unix))]
    m.insert("ulimit".into(), builtin::<ulimit::ULimitCommand, S>());
    #[cfg(all(feature = "builtin.umask", unix))]
    m.insert("umask".into(), builtin::<umask::UmaskCommand, S>());
    #[cfg(feature = "builtin.unalias")]
    m.insert("unalias".into(), builtin::<unalias::UnaliasCommand, S>());
    #[cfg(feature = "builtin.wait")]
    m.insert("wait".into(), builtin::<wait::WaitCommand, S>());

    #[cfg(feature = "builtin.fc")]
    m.insert("fc".into(), builtin::<fc::FcCommand, S>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        m.insert(
            "builtin".into(),
            raw_arg_builtin::<builtin_::BuiltinCommand, S>(),
        );
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "declare".into(),
            decl_builtin::<declare::DeclareCommand, S>(),
        );
        #[cfg(feature = "builtin.echo")]
        m.insert("echo".into(), builtin::<echo::EchoCommand, S>());
        #[cfg(feature = "builtin.enable")]
        m.insert("enable".into(), builtin::<enable::EnableCommand, S>());
        #[cfg(feature = "builtin.let")]
        m.insert("let".into(), builtin::<let_::LetCommand, S>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("mapfile".into(), builtin::<mapfile::MapFileCommand, S>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("readarray".into(), builtin::<mapfile::MapFileCommand, S>());
        #[cfg(all(feature = "builtin.printf", any(unix, windows)))]
        m.insert("printf".into(), builtin::<printf::PrintfCommand, S>());
        #[cfg(feature = "builtin.shopt")]
        m.insert("shopt".into(), builtin::<shopt::ShoptCommand, S>());
        #[cfg(feature = "builtin.dot")]
        m.insert("source".into(), builtin::<dot::DotCommand, S>().special());
        #[cfg(all(feature = "builtin.suspend", unix))]
        m.insert("suspend".into(), builtin::<suspend::SuspendCommand, S>());
        #[cfg(feature = "builtin.test")]
        m.insert("test".into(), builtin::<test::TestCommand, S>());
        #[cfg(feature = "builtin.test")]
        m.insert("[".into(), builtin::<test::TestCommand, S>());
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "typeset".into(),
            decl_builtin::<declare::DeclareCommand, S>(),
        );

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        m.insert("complete".into(), builtin::<complete::CompleteCommand, S>());
        #[cfg(feature = "builtin.compgen")]
        m.insert("compgen".into(), builtin::<complete::CompGenCommand, S>());
        #[cfg(feature = "builtin.compopt")]
        m.insert("compopt".into(), builtin::<complete::CompOptCommand, S>());

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        m.insert("dirs".into(), builtin::<dirs::DirsCommand, S>());
        #[cfg(feature = "builtin.popd")]
        m.insert("popd".into(), builtin::<popd::PopdCommand, S>());
        #[cfg(feature = "builtin.pushd")]
        m.insert("pushd".into(), builtin::<pushd::PushdCommand, S>());

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        m.insert("bind".into(), builtin::<bind::BindCommand, S>());

        // History
        #[cfg(feature = "builtin.history")]
        m.insert("history".into(), builtin::<history::HistoryCommand, S>());

        #[cfg(feature = "builtin.caller")]
        m.insert("caller".into(), builtin::<caller::CallerCommand, S>());

        // TODO(disown): implement disown builtin
        m.insert("disown".into(), builtin::<unimp::UnimplementedCommand, S>());

        // TODO(logout): implement logout builtin
        m.insert("logout".into(), builtin::<unimp::UnimplementedCommand, S>());
    }

    m
}
