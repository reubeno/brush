use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

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
pub fn default_builtins(set: BuiltinSet) -> HashMap<String, builtins::Registration> {
    let mut m = HashMap::<String, builtins::Registration>::new();

    //
    // POSIX special builtins
    //
    // N.B. There seems to be some inconsistency as to whether 'times'
    // should be a special built-in.
    //

    m.insert("break".into(), builtin::<break_::BreakCommand>().special());
    m.insert(
        ":".into(),
        simple_builtin::<colon::ColonCommand>().special(),
    );
    m.insert(
        "continue".into(),
        builtin::<continue_::ContinueCommand>().special(),
    );
    m.insert(".".into(), builtin::<dot::DotCommand>().special());
    m.insert("eval".into(), builtin::<eval::EvalCommand>().special());
    #[cfg(unix)]
    m.insert("exec".into(), builtin::<exec::ExecCommand>().special());
    m.insert("exit".into(), builtin::<exit::ExitCommand>().special());
    m.insert(
        "export".into(),
        decl_builtin::<export::ExportCommand>().special(),
    );
    m.insert(
        "return".into(),
        builtin::<return_::ReturnCommand>().special(),
    );
    m.insert("set".into(), builtin::<set::SetCommand>().special());
    m.insert("shift".into(), builtin::<shift::ShiftCommand>().special());
    m.insert("trap".into(), builtin::<trap::TrapCommand>().special());
    m.insert("unset".into(), builtin::<unset::UnsetCommand>().special());

    m.insert(
        "readonly".into(),
        decl_builtin::<declare::DeclareCommand>().special(),
    );
    m.insert("times".into(), builtin::<times::TimesCommand>().special());

    //
    // Non-special builtins
    //

    m.insert("alias".into(), builtin::<alias::AliasCommand>()); // TODO: should be exec_declaration_builtin
    m.insert("bg".into(), builtin::<bg::BgCommand>());
    m.insert("cd".into(), builtin::<cd::CdCommand>());
    m.insert("command".into(), builtin::<command::CommandCommand>());
    m.insert("false".into(), builtin::<false_::FalseCommand>());
    m.insert("fg".into(), builtin::<fg::FgCommand>());
    m.insert("getopts".into(), builtin::<getopts::GetOptsCommand>());
    m.insert("hash".into(), builtin::<hash::HashCommand>());
    m.insert("help".into(), builtin::<help::HelpCommand>());
    m.insert("jobs".into(), builtin::<jobs::JobsCommand>());
    #[cfg(unix)]
    m.insert("kill".into(), builtin::<kill::KillCommand>());
    m.insert("local".into(), decl_builtin::<declare::DeclareCommand>());
    m.insert("pwd".into(), builtin::<pwd::PwdCommand>());
    m.insert("read".into(), builtin::<read::ReadCommand>());
    m.insert("true".into(), builtin::<true_::TrueCommand>());
    m.insert("type".into(), builtin::<type_::TypeCommand>());
    #[cfg(unix)]
    m.insert("ulimit".into(), builtin::<ulimit::ULimitCommand>());
    #[cfg(unix)]
    m.insert("umask".into(), builtin::<umask::UmaskCommand>());
    m.insert("unalias".into(), builtin::<unalias::UnaliasCommand>());
    m.insert("wait".into(), builtin::<wait::WaitCommand>());

    // TODO: implement fc builtin; should be done after history.
    m.insert("fc".into(), builtin::<unimp::UnimplementedCommand>());

    if matches!(set, BuiltinSet::BashMode) {
        m.insert(
            "builtin".into(),
            raw_arg_builtin::<builtin_::BuiltinCommand>(),
        );
        m.insert("declare".into(), decl_builtin::<declare::DeclareCommand>());
        m.insert("echo".into(), builtin::<echo::EchoCommand>());
        m.insert("enable".into(), builtin::<enable::EnableCommand>());
        m.insert("let".into(), builtin::<let_::LetCommand>());
        m.insert("mapfile".into(), builtin::<mapfile::MapFileCommand>());
        m.insert("readarray".into(), builtin::<mapfile::MapFileCommand>());
        #[cfg(any(unix, windows))]
        m.insert("printf".into(), builtin::<printf::PrintfCommand>());
        m.insert("shopt".into(), builtin::<shopt::ShoptCommand>());
        m.insert("source".into(), builtin::<dot::DotCommand>().special());
        #[cfg(unix)]
        m.insert("suspend".into(), builtin::<suspend::SuspendCommand>());
        m.insert("test".into(), builtin::<test::TestCommand>());
        m.insert("[".into(), builtin::<test::TestCommand>());
        m.insert("typeset".into(), decl_builtin::<declare::DeclareCommand>());

        // Completion builtins
        m.insert("complete".into(), builtin::<complete::CompleteCommand>());
        m.insert("compgen".into(), builtin::<complete::CompGenCommand>());
        m.insert("compopt".into(), builtin::<complete::CompOptCommand>());

        // Dir stack builtins
        m.insert("dirs".into(), builtin::<dirs::DirsCommand>());
        m.insert("popd".into(), builtin::<popd::PopdCommand>());
        m.insert("pushd".into(), builtin::<pushd::PushdCommand>());

        // Input configuration builtins
        m.insert("bind".into(), builtin::<bind::BindCommand>());

        // History
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
    m.insert("brushinfo".into(), builtin::<brushinfo::BrushInfoCommand>());

    m
}
