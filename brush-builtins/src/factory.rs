use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

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
pub fn default_builtins(set: BuiltinSet) -> HashMap<String, builtins::Registration> {
    let mut m = HashMap::<String, builtins::Registration>::new();

    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    #[cfg(feature = "builtin.break")]
    m.insert("break".into(), builtin::<break_::BreakCommand>().special());
    #[cfg(feature = "builtin.colon")]
    m.insert(
        ":".into(),
        simple_builtin::<colon::ColonCommand>().special(),
    );
    #[cfg(feature = "builtin.continue")]
    m.insert(
        "continue".into(),
        builtin::<continue_::ContinueCommand>().special(),
    );
    #[cfg(feature = "builtin.dot")]
    m.insert(".".into(), builtin::<dot::DotCommand>().special());
    #[cfg(feature = "builtin.eval")]
    m.insert("eval".into(), builtin::<eval::EvalCommand>().special());
    #[cfg(all(feature = "builtin.exec", unix))]
    m.insert("exec".into(), builtin::<exec::ExecCommand>().special());
    #[cfg(feature = "builtin.exit")]
    m.insert("exit".into(), builtin::<exit::ExitCommand>().special());
    #[cfg(feature = "builtin.export")]
    m.insert(
        "export".into(),
        decl_builtin::<export::ExportCommand>().special(),
    );
    #[cfg(feature = "builtin.return")]
    m.insert(
        "return".into(),
        builtin::<return_::ReturnCommand>().special(),
    );
    #[cfg(feature = "builtin.set")]
    m.insert("set".into(), builtin::<set::SetCommand>().special());
    #[cfg(feature = "builtin.shift")]
    m.insert("shift".into(), builtin::<shift::ShiftCommand>().special());
    #[cfg(feature = "builtin.trap")]
    m.insert("trap".into(), builtin::<trap::TrapCommand>().special());
    #[cfg(feature = "builtin.unset")]
    m.insert("unset".into(), builtin::<unset::UnsetCommand>().special());

    #[cfg(feature = "builtin.declare")]
    m.insert(
        "readonly".into(),
        decl_builtin::<declare::DeclareCommand>().special(),
    );
    #[cfg(feature = "builtin.times")]
    m.insert("times".into(), builtin::<times::TimesCommand>().special());

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    m.insert("alias".into(), builtin::<alias::AliasCommand>()); // TODO: should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    m.insert("bg".into(), builtin::<bg::BgCommand>());
    #[cfg(feature = "builtin.cd")]
    m.insert("cd".into(), builtin::<cd::CdCommand>());
    #[cfg(feature = "builtin.command")]
    m.insert("command".into(), builtin::<command::CommandCommand>());
    #[cfg(feature = "builtin.false")]
    m.insert("false".into(), builtin::<false_::FalseCommand>());
    #[cfg(feature = "builtin.fg")]
    m.insert("fg".into(), builtin::<fg::FgCommand>());
    #[cfg(feature = "builtin.getopts")]
    m.insert("getopts".into(), builtin::<getopts::GetOptsCommand>());
    #[cfg(feature = "builtin.hash")]
    m.insert("hash".into(), builtin::<hash::HashCommand>());
    #[cfg(feature = "builtin.help")]
    m.insert("help".into(), builtin::<help::HelpCommand>());
    #[cfg(feature = "builtin.jobs")]
    m.insert("jobs".into(), builtin::<jobs::JobsCommand>());
    #[cfg(all(feature = "builtin.kill", unix))]
    m.insert("kill".into(), builtin::<kill::KillCommand>());
    #[cfg(feature = "builtin.declare")]
    m.insert("local".into(), decl_builtin::<declare::DeclareCommand>());
    #[cfg(feature = "builtin.pwd")]
    m.insert("pwd".into(), builtin::<pwd::PwdCommand>());
    #[cfg(feature = "builtin.read")]
    m.insert("read".into(), builtin::<read::ReadCommand>());
    #[cfg(feature = "builtin.true")]
    m.insert("true".into(), builtin::<true_::TrueCommand>());
    #[cfg(feature = "builtin.type")]
    m.insert("type".into(), builtin::<type_::TypeCommand>());
    #[cfg(all(feature = "builtin.ulimit", unix))]
    m.insert("ulimit".into(), builtin::<ulimit::ULimitCommand>());
    #[cfg(all(feature = "builtin.umask", unix))]
    m.insert("umask".into(), builtin::<umask::UmaskCommand>());
    #[cfg(feature = "builtin.unalias")]
    m.insert("unalias".into(), builtin::<unalias::UnaliasCommand>());
    #[cfg(feature = "builtin.wait")]
    m.insert("wait".into(), builtin::<wait::WaitCommand>());

    #[cfg(feature = "builtin.fc")]
    m.insert("fc".into(), builtin::<fc::FcCommand>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        m.insert(
            "builtin".into(),
            raw_arg_builtin::<builtin_::BuiltinCommand>(),
        );
        #[cfg(feature = "builtin.declare")]
        m.insert("declare".into(), decl_builtin::<declare::DeclareCommand>());
        #[cfg(feature = "builtin.echo")]
        m.insert("echo".into(), builtin::<echo::EchoCommand>());
        #[cfg(feature = "builtin.enable")]
        m.insert("enable".into(), builtin::<enable::EnableCommand>());
        #[cfg(feature = "builtin.let")]
        m.insert("let".into(), builtin::<let_::LetCommand>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("mapfile".into(), builtin::<mapfile::MapFileCommand>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert("readarray".into(), builtin::<mapfile::MapFileCommand>());
        #[cfg(all(feature = "builtin.printf", any(unix, windows)))]
        m.insert("printf".into(), builtin::<printf::PrintfCommand>());
        #[cfg(feature = "builtin.shopt")]
        m.insert("shopt".into(), builtin::<shopt::ShoptCommand>());
        #[cfg(feature = "builtin.dot")]
        m.insert("source".into(), builtin::<dot::DotCommand>().special());
        #[cfg(all(feature = "builtin.suspend", unix))]
        m.insert("suspend".into(), builtin::<suspend::SuspendCommand>());
        #[cfg(feature = "builtin.test")]
        m.insert("test".into(), builtin::<test::TestCommand>());
        #[cfg(feature = "builtin.test")]
        m.insert("[".into(), builtin::<test::TestCommand>());
        #[cfg(feature = "builtin.declare")]
        m.insert("typeset".into(), decl_builtin::<declare::DeclareCommand>());

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        m.insert("complete".into(), builtin::<complete::CompleteCommand>());
        #[cfg(feature = "builtin.compgen")]
        m.insert("compgen".into(), builtin::<complete::CompGenCommand>());
        #[cfg(feature = "builtin.compopt")]
        m.insert("compopt".into(), builtin::<complete::CompOptCommand>());

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        m.insert("dirs".into(), builtin::<dirs::DirsCommand>());
        #[cfg(feature = "builtin.popd")]
        m.insert("popd".into(), builtin::<popd::PopdCommand>());
        #[cfg(feature = "builtin.pushd")]
        m.insert("pushd".into(), builtin::<pushd::PushdCommand>());

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        m.insert("bind".into(), builtin::<bind::BindCommand>());

        // History
        #[cfg(feature = "builtin.history")]
        m.insert("history".into(), builtin::<history::HistoryCommand>());

        // TODO: implement caller builtin
        m.insert("caller".into(), builtin::<unimp::UnimplementedCommand>());

        // TODO: implement disown builtin
        m.insert("disown".into(), builtin::<unimp::UnimplementedCommand>());

        // TODO: implement logout builtin
        m.insert("logout".into(), builtin::<unimp::UnimplementedCommand>());
    }

    //
    // Brush-specific builtins.
    //
    #[cfg(feature = "builtin.brushinfo")]
    m.insert("brushinfo".into(), builtin::<brushinfo::BrushInfoCommand>());

    m
}
