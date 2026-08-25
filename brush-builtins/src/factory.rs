use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use brush_core::builtins::{
    self, builtin, decl_builtin, raw_arg_builtin, simple_builtin, spec_builtin,
};

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
        spec_builtin::<break_::BreakCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.colon")]
    m.insert(
        ":".into(),
        simple_builtin::<colon::ColonCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.continue")]
    m.insert(
        "continue".into(),
        spec_builtin::<continue_::ContinueCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.dot")]
    m.insert(".".into(), spec_builtin::<dot::DotCommand, SE>().special());
    #[cfg(feature = "builtin.eval")]
    m.insert(
        "eval".into(),
        spec_builtin::<eval::EvalCommand, SE>().special(),
    );
    #[cfg(all(feature = "builtin.exec", unix))]
    m.insert(
        "exec".into(),
        spec_builtin::<exec::ExecCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.exit")]
    m.insert(
        "exit".into(),
        spec_builtin::<exit::ExitCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.export")]
    m.insert(
        "export".into(),
        spec_builtin::<export::ExportCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.return")]
    m.insert(
        "return".into(),
        spec_builtin::<return_::ReturnCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.set")]
    m.insert(
        "set".into(),
        spec_builtin::<set::SetCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.shift")]
    m.insert(
        "shift".into(),
        spec_builtin::<shift::ShiftCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.trap")]
    m.insert(
        "trap".into(),
        spec_builtin::<trap::TrapCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.unset")]
    m.insert(
        "unset".into(),
        spec_builtin::<unset::UnsetCommand, SE>().special(),
    );

    #[cfg(feature = "builtin.declare")]
    m.insert(
        "readonly".into(),
        spec_builtin::<declare::DeclareCommand, SE>().special(),
    );
    #[cfg(feature = "builtin.times")]
    m.insert(
        "times".into(),
        spec_builtin::<times::TimesCommand, SE>().special(),
    );

    //
    // Non-special builtins
    //

    #[cfg(feature = "builtin.alias")]
    m.insert("alias".into(), spec_builtin::<alias::AliasCommand, SE>()); // TODO(alias): should be exec_declaration_builtin
    #[cfg(feature = "builtin.bg")]
    m.insert("bg".into(), spec_builtin::<bg::BgCommand, SE>());
    #[cfg(feature = "builtin.cd")]
    m.insert("cd".into(), spec_builtin::<cd::CdCommand, SE>());
    #[cfg(feature = "builtin.command")]
    m.insert(
        "command".into(),
        spec_builtin::<command::CommandCommand, SE>(),
    );
    #[cfg(feature = "builtin.false")]
    m.insert("false".into(), simple_builtin::<false_::FalseCommand, SE>());
    #[cfg(feature = "builtin.fg")]
    m.insert("fg".into(), spec_builtin::<fg::FgCommand, SE>());
    #[cfg(feature = "builtin.getopts")]
    m.insert(
        "getopts".into(),
        spec_builtin::<getopts::GetOptsCommand, SE>(),
    );
    #[cfg(feature = "builtin.hash")]
    m.insert("hash".into(), spec_builtin::<hash::HashCommand, SE>());
    #[cfg(feature = "builtin.help")]
    m.insert("help".into(), spec_builtin::<help::HelpCommand, SE>());
    #[cfg(feature = "builtin.jobs")]
    m.insert("jobs".into(), spec_builtin::<jobs::JobsCommand, SE>());
    #[cfg(all(feature = "builtin.kill", unix))]
    m.insert("kill".into(), spec_builtin::<kill::KillCommand, SE>());
    #[cfg(feature = "builtin.declare")]
    m.insert(
        "local".into(),
        spec_builtin::<declare::DeclareCommand, SE>(),
    );
    #[cfg(feature = "builtin.pwd")]
    m.insert("pwd".into(), builtin::<pwd::PwdCommand, SE>());
    #[cfg(feature = "builtin.read")]
    m.insert("read".into(), spec_builtin::<read::ReadCommand, SE>());
    #[cfg(feature = "builtin.true")]
    m.insert("true".into(), simple_builtin::<true_::TrueCommand, SE>());
    #[cfg(feature = "builtin.type")]
    m.insert("type".into(), spec_builtin::<type_::TypeCommand, SE>());
    #[cfg(all(feature = "builtin.ulimit", unix))]
    m.insert("ulimit".into(), spec_builtin::<ulimit::ULimitCommand, SE>());
    #[cfg(all(feature = "builtin.umask", unix))]
    m.insert("umask".into(), spec_builtin::<umask::UmaskCommand, SE>());
    #[cfg(feature = "builtin.unalias")]
    m.insert(
        "unalias".into(),
        spec_builtin::<unalias::UnaliasCommand, SE>(),
    );
    #[cfg(feature = "builtin.wait")]
    m.insert("wait".into(), spec_builtin::<wait::WaitCommand, SE>());

    #[cfg(feature = "builtin.fc")]
    m.insert("fc".into(), spec_builtin::<fc::FcCommand, SE>());

    if matches!(set, BuiltinSet::BashMode) {
        #[cfg(feature = "builtin.builtin")]
        m.insert(
            "builtin".into(),
            spec_builtin::<builtin_::BuiltinCommand, SE>(),
        );
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "declare".into(),
            spec_builtin::<declare::DeclareCommand, SE>(),
        );
        #[cfg(feature = "builtin.echo")]
        m.insert("echo".into(), spec_builtin::<echo::EchoCommand, SE>());
        #[cfg(feature = "builtin.enable")]
        m.insert("enable".into(), spec_builtin::<enable::EnableCommand, SE>());
        #[cfg(feature = "builtin.let")]
        m.insert("let".into(), spec_builtin::<let_::LetCommand, SE>());
        #[cfg(feature = "builtin.mapfile")]
        m.insert(
            "mapfile".into(),
            spec_builtin::<mapfile::MapFileCommand, SE>(),
        );
        #[cfg(feature = "builtin.mapfile")]
        m.insert(
            "readarray".into(),
            spec_builtin::<mapfile::MapFileCommand, SE>(),
        );
        #[cfg(all(feature = "builtin.printf", any(unix, windows)))]
        m.insert("printf".into(), spec_builtin::<printf::PrintfCommand, SE>());
        #[cfg(feature = "builtin.shopt")]
        m.insert("shopt".into(), spec_builtin::<shopt::ShoptCommand, SE>());
        #[cfg(feature = "builtin.dot")]
        m.insert(
            "source".into(),
            spec_builtin::<dot::DotCommand, SE>().special(),
        );
        #[cfg(all(feature = "builtin.suspend", unix))]
        m.insert(
            "suspend".into(),
            spec_builtin::<suspend::SuspendCommand, SE>(),
        );
        #[cfg(feature = "builtin.test")]
        m.insert("test".into(), spec_builtin::<test::TestCommand, SE>());
        #[cfg(feature = "builtin.test")]
        m.insert("[".into(), spec_builtin::<test::TestCommand, SE>());
        #[cfg(feature = "builtin.declare")]
        m.insert(
            "typeset".into(),
            spec_builtin::<declare::DeclareCommand, SE>(),
        );

        // Completion builtins
        #[cfg(feature = "builtin.complete")]
        m.insert(
            "complete".into(),
            spec_builtin::<complete::CompleteCommand, SE>(),
        );
        #[cfg(feature = "builtin.compgen")]
        m.insert(
            "compgen".into(),
            spec_builtin::<complete::CompGenCommand, SE>(),
        );
        #[cfg(feature = "builtin.compopt")]
        m.insert(
            "compopt".into(),
            spec_builtin::<complete::CompOptCommand, SE>(),
        );

        // Dir stack builtins
        #[cfg(feature = "builtin.dirs")]
        m.insert("dirs".into(), spec_builtin::<dirs::DirsCommand, SE>());
        #[cfg(feature = "builtin.popd")]
        m.insert("popd".into(), spec_builtin::<popd::PopdCommand, SE>());
        #[cfg(feature = "builtin.pushd")]
        m.insert("pushd".into(), spec_builtin::<pushd::PushdCommand, SE>());

        // Input configuration builtins
        #[cfg(feature = "builtin.bind")]
        m.insert("bind".into(), spec_builtin::<bind::BindCommand, SE>());

        // History
        #[cfg(feature = "builtin.history")]
        m.insert(
            "history".into(),
            spec_builtin::<history::HistoryCommand, SE>(),
        );

        #[cfg(feature = "builtin.caller")]
        m.insert("caller".into(), spec_builtin::<caller::CallerCommand, SE>());

        // TODO(disown): implement disown builtin
        m.insert(
            "disown".into(),
            spec_builtin::<unimp::UnimplementedCommand, SE>(),
        );

        // TODO(logout): implement logout builtin
        m.insert(
            "logout".into(),
            spec_builtin::<unimp::UnimplementedCommand, SE>(),
        );
    }

    m
}
