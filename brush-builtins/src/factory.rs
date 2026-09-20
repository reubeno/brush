use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use brush_core::builtins::{self, builtin};

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
pub fn default_builtins<SE: brush_core::ShellExtensions>(
    set: BuiltinSet,
) -> HashMap<String, builtins::Registration<SE>> {
    let mut m = HashMap::<String, builtins::Registration<SE>>::new();

    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    #[cfg(feature = "builtin.break")]
    m.insert(
        "break".into(),
        builtin::<break_::BreakCommand, _>().special(),
    );
    #[cfg(feature = "builtin.colon")]
    m.insert(":".into(), builtin::<colon::ColonCommand, _>().special());
    #[cfg(feature = "builtin.continue")]
    m.insert(
        "continue".into(),
        builtin::<continue_::ContinueCommand, _>().special(),
    );
    #[cfg(feature = "builtin.dot")]
    m.insert(".".into(), builtin::<dot::DotCommand, _>().special());
    #[cfg(feature = "builtin.eval")]
    m.insert("eval".into(), builtin::<eval::EvalCommand, _>().special());
    #[cfg(all(feature = "builtin.exec", unix))]
    m.insert("exec".into(), builtin::<exec::ExecCommand, _>().special());
    #[cfg(feature = "builtin.exit")]
    m.insert("exit".into(), builtin::<exit::ExitCommand, _>().special());
    #[cfg(feature = "builtin.export")]
    m.insert(
        "export".into(),
        builtin::<export::ExportCommand, _>().special(),
    );
    #[cfg(feature = "builtin.return")]
    m.insert(
        "return".into(),
        builtin::<return_::ReturnCommand, _>().special(),
    );
    #[cfg(feature = "builtin.set")]
    m.insert("set".into(), builtin::<set::SetCommand, _>().special());
    #[cfg(feature = "builtin.shift")]
    m.insert(
        "shift".into(),
        builtin::<shift::ShiftCommand, _>().special(),
    );
    #[cfg(feature = "builtin.trap")]
    m.insert("trap".into(), builtin::<trap::TrapCommand, _>().special());
    #[cfg(feature = "builtin.unset")]
    m.insert(
        "unset".into(),
        builtin::<unset::UnsetCommand, _>().special(),
    );

    #[cfg(feature = "builtin.declare")]
    m.insert(
        "readonly".into(),
        builtin::<declare::DeclareCommand, _>().special(),
    );
    #[cfg(feature = "builtin.times")]
    m.insert(
        "times".into(),
        builtin::<times::TimesCommand, _>().special(),
    );

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    m.insert("alias".into(), builtin::<alias::AliasCommand, _>()); // TODO(alias): should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    m.insert("bg".into(), builtin::<bg::BgCommand, _>());
    #[cfg(feature = "builtin.cd")]
    m.insert("cd".into(), builtin::<cd::CdCommand, _>());
    #[cfg(feature = "builtin.command")]
    m.insert("command".into(), builtin::<command::CommandCommand, _>());
    #[cfg(feature = "builtin.false")]
    m.insert("false".into(), builtin::<false_::FalseCommand, _>());
    #[cfg(feature = "builtin.fg")]
    m.insert("fg".into(), builtin::<fg::FgCommand, _>());
    #[cfg(feature = "builtin.getopts")]
    m.insert("getopts".into(), builtin::<getopts::GetOptsCommand, _>());
    #[cfg(feature = "builtin.hash")]
    m.insert("hash".into(), builtin::<hash::HashCommand, _>());
    #[cfg(feature = "builtin.help")]
    m.insert("help".into(), builtin::<help::HelpCommand, _>());
    #[cfg(feature = "builtin.jobs")]
    m.insert("jobs".into(), builtin::<jobs::JobsCommand, _>());
    #[cfg(all(feature = "builtin.kill", unix))]
    m.insert("kill".into(), builtin::<kill::KillCommand, _>());
    #[cfg(feature = "builtin.declare")]
    m.insert("local".into(), builtin::<declare::DeclareCommand, _>());
    #[cfg(feature = "builtin.pwd")]
    m.insert("pwd".into(), builtin::<pwd::PwdCommand, _>());
    #[cfg(feature = "builtin.read")]
    m.insert("read".into(), builtin::<read::ReadCommand, _>());
    #[cfg(feature = "builtin.true")]
    m.insert("true".into(), builtin::<true_::TrueCommand, _>());
    #[cfg(feature = "builtin.type")]
    m.insert("type".into(), builtin::<type_::TypeCommand, _>());
    #[cfg(all(feature = "builtin.ulimit", unix))]
    m.insert("ulimit".into(), builtin::<ulimit::ULimitCommand, _>());
    #[cfg(all(feature = "builtin.umask", unix))]
    m.insert("umask".into(), builtin::<umask::UmaskCommand, _>());
    #[cfg(feature = "builtin.unalias")]
    m.insert("unalias".into(), builtin::<unalias::UnaliasCommand, _>());
    #[cfg(feature = "builtin.wait")]
    m.insert("wait".into(), builtin::<wait::WaitCommand, _>());

    #[cfg(feature = "builtin.fc")]
    m.insert("fc".into(), builtin::<fc::FcCommand, _>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        m.insert("builtin".into(), builtin::<builtin_::BuiltinCommand, _>());
        #[cfg(feature = "builtin.declare")]
        m.insert("declare".into(), builtin::<declare::DeclareCommand, _>());
        #[cfg(feature = "builtin.echo")]
        m.insert("echo".into(), builtin::<echo::EchoCommand, _>());
        #[cfg(feature = "builtin.enable")]
        m.insert("enable".into(), builtin::<enable::EnableCommand, _>());
        #[cfg(feature = "builtin.let")]
        m.insert("let".into(), builtin::<let_::LetCommand, _>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("mapfile".into(), builtin::<mapfile::MapFileCommand, _>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("readarray".into(), builtin::<mapfile::MapFileCommand, _>());
        #[cfg(all(feature = "builtin.printf", any(unix, windows)))]
        m.insert("printf".into(), builtin::<printf::PrintfCommand, _>());
        #[cfg(feature = "builtin.shopt")]
        m.insert("shopt".into(), builtin::<shopt::ShoptCommand, _>());
        #[cfg(feature = "builtin.dot")]
        m.insert("source".into(), builtin::<dot::DotCommand, _>().special());
        #[cfg(all(feature = "builtin.suspend", unix))]
        m.insert("suspend".into(), builtin::<suspend::SuspendCommand, _>());
        #[cfg(feature = "builtin.test")]
        m.insert("test".into(), builtin::<test::TestCommand, _>());
        #[cfg(feature = "builtin.test")]
        m.insert("[".into(), builtin::<test::TestCommand, _>());
        #[cfg(feature = "builtin.declare")]
        m.insert("typeset".into(), builtin::<declare::DeclareCommand, _>());

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        m.insert("complete".into(), builtin::<complete::CompleteCommand, _>());
        #[cfg(feature = "builtin.compgen")]
        m.insert("compgen".into(), builtin::<complete::CompGenCommand, _>());
        #[cfg(feature = "builtin.compopt")]
        m.insert("compopt".into(), builtin::<complete::CompOptCommand, _>());

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        m.insert("dirs".into(), builtin::<dirs::DirsCommand, _>());
        #[cfg(feature = "builtin.popd")]
        m.insert("popd".into(), builtin::<popd::PopdCommand, _>());
        #[cfg(feature = "builtin.pushd")]
        m.insert("pushd".into(), builtin::<pushd::PushdCommand, _>());

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        m.insert("bind".into(), builtin::<bind::BindCommand, _>());

        // History
        #[cfg(feature = "builtin.history")]
        m.insert("history".into(), builtin::<history::HistoryCommand, _>());

        #[cfg(feature = "builtin.caller")]
        m.insert("caller".into(), builtin::<caller::CallerCommand, _>());

        // TODO(disown): implement disown builtin
        m.insert("disown".into(), builtin::<unimp::UnimplementedCommand, _>());

        // TODO(logout): implement logout builtin
        m.insert("logout".into(), builtin::<unimp::UnimplementedCommand, _>());
    }

    m
}
